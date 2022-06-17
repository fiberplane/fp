use abort_on_drop::ChildTask;
use anyhow::Result;
use blocking::{unblock, Unblock};
use crossterm::{
    event::{Event, EventStream},
    terminal,
};
use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use memchr::memmem;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::{
    borrow::Borrow,
    cell::{Ref, RefCell},
    cmp,
    io::{BufRead, Cursor, Read},
};
use vmap::io::{Ring, SeqRead, SeqWrite};

pub struct RawGuard {
    was_enabled: bool,
}

impl RawGuard {
    #[allow(dead_code)]
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

#[derive(Debug)]
pub enum PtyOutput<'a> {
    Data(Ref<'a, [u8]>),
    PromptStart,
    PromptEnd,
    PromptContinue,
}

impl PartialEq for PtyOutput<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Data(l0), Self::Data(r0)) => (**l0.borrow()) == (**r0.borrow()),
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

#[derive(PartialEq, Eq)]
enum State {
    Process,
    Read,
    Consume(usize),
}

struct TerminalExtractor<R: tokio::io::AsyncReadExt> {
    buffer: RefCell<vmap::io::Ring>,
    state: State,
    reader: R,
}

pub struct PtyTerminal<R: AsyncReadExt> {
    buffer: RefCell<vmap::io::Ring>,
    state: State,
    child_stdout: R,
    stdin_task: ChildTask<Result<()>>,
    resize_task: ChildTask<Result<()>>,
}

const START_PROMPT_BYTES: &[u8] = "\u{200b}\u{200b}".as_bytes();
const END_PROMPT_BYTES: &[u8] = "\u{200e}\u{200e}".as_bytes();

impl PtyTerminal<Unblock<Box<dyn Read + Send>>> {
    pub async fn new(cmd: CommandBuilder) -> Result<Self> {
        let (cols, rows) = terminal::size()?;
        let pty = native_pty_system().openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        // Move the slave to another thread to block and spawn a
        // command.
        // Note that this implicitly drops slave and closes out
        // file handles which is important to avoid deadlock
        // when waiting for the child process!
        let slave = pty.slave;
        let mut child = tokio::task::spawn_blocking(move || slave.spawn_command(cmd)).await??;

        //let mut child_waiter = unblock(move || child.wait());

        Ok(Self {
            child_stdout: Unblock::new(pty.master.try_clone_reader()?),
            state: State::Read,
            buffer: RefCell::new(vmap::io::Ring::new(4096)?),
            stdin_task: ChildTask::from(tokio::spawn(Self::forward_stdin(Unblock::new(
                pty.master.try_clone_writer()?,
            )))),
            resize_task: ChildTask::from(tokio::spawn(Self::forward_resize(pty.master))),
        })
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

    async fn forward_stdin(mut writer: impl AsyncWriteExt + Unpin) -> Result<()> {
        let mut stdin = Unblock::new(std::io::stdin());
        let mut buf = [0u8; 1024];

        while let Ok(bytes) = stdin.read(&mut buf).await {
            writer.write_all(&buf[0..bytes]).await?;
        }

        Ok(())
    }
}

impl<R: tokio::io::AsyncReadExt + Unpin> TerminalExtractor<R> {
    pub fn new(reader: R) -> Result<Self> {
        Ok(Self {
            buffer: RefCell::new(Ring::new(4096)?),
            state: State::Read,
            reader,
        })
    }

    pub async fn next(&mut self) -> Result<PtyOutput<'_>> {
        loop {
            match self.state {
                State::Read => {
                    let mut buf = self.buffer.borrow_mut();

                    let buffered = {
                        let slice = buf.as_write_slice(usize::MAX);
                        self.reader.read(slice).await?
                    };
                    buf.feed(buffered);
                    self.state = State::Process;
                }
                State::Process => {
                    let data = Ref::map(self.buffer.borrow(), |buf| buf.as_read_slice(usize::MAX));

                    let start_prompt_pos = memmem::find(&*data, START_PROMPT_BYTES);
                    let end_prompt_pos = memmem::find(&*data, END_PROMPT_BYTES);

                    return Ok(match (start_prompt_pos, end_prompt_pos) {
                        (Some(0), _) => {
                            self.state = State::Consume(START_PROMPT_BYTES.len());
                            PtyOutput::PromptStart
                        }
                        (_, Some(0)) => {
                            self.state = State::Consume(END_PROMPT_BYTES.len());
                            PtyOutput::PromptEnd
                        }
                        (Some(pos), None) | (None, Some(pos)) => {
                            self.state = State::Consume(pos);
                            PtyOutput::Data(Ref::map(data, |slice| &slice[..pos]))
                        }
                        (Some(start), Some(end)) => {
                            let valid_data_len = cmp::min(start, end);
                            self.state = State::Consume(valid_data_len);
                            PtyOutput::Data(Ref::map(data, |slice| &slice[..valid_data_len]))
                        }
                        (None, None) => {
                            let data_len = data.len();
                            let num_partial_start_bytes_match = (1..START_PROMPT_BYTES.len())
                                .rev()
                                .find(|i| START_PROMPT_BYTES.starts_with(&data[data_len - i..]))
                                .unwrap_or_default();
                            let num_partial_end_bytes_match = (1..END_PROMPT_BYTES.len())
                                .rev()
                                .find(|i| START_PROMPT_BYTES.starts_with(&data[data_len - i..]))
                                .unwrap_or_default();

                            let max_bytes = cmp::max(
                                num_partial_start_bytes_match,
                                num_partial_end_bytes_match,
                            );

                            let valid_data_len = data_len - max_bytes;
                            self.state = State::Consume(valid_data_len);
                            PtyOutput::Data(Ref::map(data, |slice| &slice[..valid_data_len]))
                        }
                    });
                }
                State::Consume(len) => {
                    let mut buf = self.buffer.borrow_mut();
                    buf.consume(len);
                    self.state = if buf.read_len() >= START_PROMPT_BYTES.len() {
                        State::Process
                    } else {
                        State::Read
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncWriteExt;

    use super::*;
    use std::ops::Deref;

    #[tokio::test]
    async fn test_basic_extraction() {
        let mut extractor = TerminalExtractor::new(
            "some initial output here\u{200b}\u{200b}My fancy prompt>\u{200e}\u{200e}".as_bytes(),
        )
        .unwrap();

        assert_eq!(
            extractor.next().await.unwrap(),
            PtyOutput::Data(Ref::map(
                RefCell::new("some initial output here".as_bytes()).borrow(),
                Deref::deref
            )),
        );
        assert_eq!(extractor.next().await.unwrap(), PtyOutput::PromptStart);
        assert_eq!(
            extractor.next().await.unwrap(),
            PtyOutput::Data(Ref::map(
                RefCell::new("My fancy prompt>".as_bytes()).borrow(),
                Deref::deref
            )),
        );
        assert_eq!(extractor.next().await.unwrap(), PtyOutput::PromptEnd);
    }

    #[tokio::test]
    async fn test_partial_extraction() {
        let (client, mut server) = tokio::io::duplex(4096);

        server
            .write_all("some initial output here\u{200b}".as_bytes())
            .await
            .unwrap();

        let mut extractor = TerminalExtractor::new(client).unwrap();

        assert_eq!(
            extractor.next().await.unwrap(),
            PtyOutput::Data(Ref::map(
                RefCell::new("some initial output here".as_bytes()).borrow(),
                Deref::deref
            )),
        );

        server
            .write_all("\u{200b}My fancy prompt>\u{200e}\u{200e}".as_bytes())
            .await
            .unwrap();

        assert_eq!(extractor.next().await.unwrap(), PtyOutput::PromptStart);
        assert_eq!(
            extractor.next().await.unwrap(),
            PtyOutput::Data(Ref::map(
                RefCell::new("My fancy prompt>".as_bytes()).borrow(),
                Deref::deref
            )),
        );
        assert_eq!(extractor.next().await.unwrap(), PtyOutput::PromptEnd);
    }
}
