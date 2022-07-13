use super::shell_launcher::ShellLauncher;
use abort_on_drop::ChildTask;
use anyhow::Result;
use blocking::{unblock, Task, Unblock};
use crossterm::terminal;
use futures::future::Fuse;
use futures::{AsyncWriteExt, FutureExt};
use portable_pty::{native_pty_system, ExitStatus, MasterPty, PtySize};
use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};
use tracing::trace;

/// A helper that enters terminal raw mode when constructed
/// and exits raw mode when dropped if it was enabled by the
/// helper
/// Raw vs cooked mode is explained better than I can here:
/// https://stackoverflow.com/a/13104585
/// In short it enables us to forward ctrl+c and such to the
/// child process
pub struct RawGuard {
    was_enabled: bool,
}

impl RawGuard {
    pub fn new() -> Self {
        if terminal::is_raw_mode_enabled().unwrap() {
            Self { was_enabled: true }
        } else {
            terminal::enable_raw_mode().unwrap();
            Self { was_enabled: false }
        }
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        if !self.was_enabled {
            terminal::disable_raw_mode().unwrap();
        }
    }
}

/// Helper that launches the child process under a pseudo terminal (PTY)
/// https://en.wikipedia.org/wiki/Pseudoterminal
/// And forwards resizing as well as stdin to the child process
pub struct PtyTerminal {
    child_waiter: Fuse<Task<Result<ExitStatus, std::io::Error>>>,
    stdin_task: Fuse<ChildTask<Result<()>>>,
    resize_task: Fuse<ChildTask<Result<()>>>,
    _guard: RawGuard,
}

impl PtyTerminal {
    pub async fn new(
        launcher: ShellLauncher,
    ) -> Result<(Self, impl tokio::io::AsyncReadExt + Send)> {
        let guard = RawGuard::new();
        let (cols, rows) = terminal::size()?;
        let pty = native_pty_system().openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let cmd = launcher.build_command();
        let pty_follower = pty.slave;
        let pty_reader = pty.master.try_clone_reader()?;
        // Spawning and waiting for the child process to end is a blocking operation so move it to another thread
        let mut child = unblock(move || pty_follower.spawn_command(cmd)).await?;
        let child_waiter = unblock(move || child.wait()).fuse();

        Ok((
            Self {
                child_waiter,
                stdin_task: ChildTask::from(tokio::spawn(Self::forward_stdin(
                    Unblock::new(pty.master.try_clone_writer()?),
                    launcher,
                )))
                .fuse(),
                resize_task: ChildTask::from(tokio::spawn(Self::forward_resize(pty.master))).fuse(),
                _guard: guard,
            },
            Unblock::new(pty_reader).compat(),
        ))
    }

    #[cfg(windows)]
    async fn forward_resize(master: Box<dyn MasterPty + Send>) -> Result<()> {
        //unfortunately we can't use this for the unix impl because it reads from
        //stdin :(
        use crossterm::event::{Event, EventStream};
        use futures::StreamExt;
        let mut stream = EventStream::new();

        while let Some(Ok(event)) = stream.next().await {
            if let Event::Resize(cols, rows) = event {
                trace!("Sending resize event: ({},{})", cols, rows);
                master.resize(PtySize {
                    rows,
                    cols,
                    //linux specific stuff EG doesn't matter
                    pixel_width: 0,
                    pixel_height: 0,
                })?;
            }
        }
        Ok(())
    }

    #[cfg(unix)]
    async fn forward_resize(master: Box<dyn MasterPty + Send>) -> Result<()> {
        use tokio::signal::unix::{signal, SignalKind};

        let mut stream = signal(SignalKind::window_change())?;
        while stream.recv().await.is_some() {
            // spawn_blocking because terminal::size() might in a worst case scenario need to
            // launch a `tput` command
            let (cols, rows) = tokio::task::spawn_blocking(terminal::size).await??;
            trace!("Sending resize event: ({},{})", cols, rows);
            master.resize(PtySize {
                rows,
                cols,
                // not actually used: https://stackoverflow.com/a/42937269
                pixel_width: 0,
                pixel_height: 0,
            })?;
        }

        Ok(())
    }

    async fn forward_stdin(
        mut writer: impl AsyncWriteExt + Unpin,
        launcher: ShellLauncher,
    ) -> Result<()> {
        launcher.initialize_shell(&mut writer).await?;

        let mut stdin = Unblock::new(std::io::stdin()).compat();
        let mut writer = writer.compat_write();

        tokio::io::copy(&mut stdin, &mut writer).await?;

        Ok(())
    }

    pub async fn wait_close(&mut self) -> Result<()> {
        tokio::select! {
            biased;
            res = &mut self.child_waiter => {
                res?;
                Ok(())
            }
            res = &mut self.stdin_task => Ok(res??),
            res = &mut self.resize_task => Ok(res??),
        }
    }
}
