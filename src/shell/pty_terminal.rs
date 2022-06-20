use super::shell_launcher::ShellLauncher;
use abort_on_drop::ChildTask;
use anyhow::Result;
use blocking::Unblock;
use crossterm::{
    event::{Event, EventStream},
    terminal,
};
use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use portable_pty::{native_pty_system, Child, MasterPty, PtySize};
use std::io::Read;

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

pub struct PtyTerminal {
    guard: RawGuard,
    stdin_task: ChildTask<Result<()>>,
    resize_task: ChildTask<Result<()>>,
}

impl PtyTerminal {
    pub async fn new(
        launcher: ShellLauncher,
    ) -> Result<(Self, Box<dyn Child + Send + Sync>, Box<dyn Read + Send>)> {
        let (cols, rows) = terminal::size()?;
        let pty = native_pty_system().openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let cmd = launcher.build_command();
        let pty_slave = pty.slave;
        let pty_reader = pty.master.try_clone_reader()?;
        let child = tokio::task::spawn_blocking(move || pty_slave.spawn_command(cmd)).await??;
        Ok((
            Self {
                guard: RawGuard::new(),
                stdin_task: ChildTask::from(tokio::spawn(Self::forward_stdin(
                    Unblock::new(pty.master.try_clone_writer()?),
                    launcher,
                ))),
                resize_task: ChildTask::from(tokio::spawn(Self::forward_resize(pty.master))),
            },
            child,
            pty_reader,
        ))
    }

    async fn forward_resize(master: Box<dyn MasterPty + Send>) -> Result<()> {
        let mut stream = EventStream::new();

        while let Some(Ok(event)) = stream.next().await {
            if let Event::Resize(cols, rows) = event {
                master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })?;
            }
        }
        Ok(())
    }

    async fn forward_stdin(
        mut writer: impl AsyncWriteExt + Unpin,
        launcher: ShellLauncher,
    ) -> Result<()> {
        let mut stdin = Unblock::new(std::io::stdin());
        let mut buf = [0u8; 1024];

        launcher.initialize_shell(&mut writer).await?;

        while let Ok(bytes) = stdin.read(&mut buf).await {
            writer.write_all(&buf[0..bytes]).await?;
        }

        Ok(())
    }
}
