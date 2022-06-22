use anyhow::Result;
use memchr::memmem;
use std::{cmp, io::BufRead};
use tracing::{instrument, trace};
use vmap::io::{Ring, SeqRead, SeqWrite};

#[derive(Debug, PartialEq, Eq)]
enum State {
    Process,
    Read,
    Consume(usize),
}

#[derive(Debug, PartialEq)]
pub enum PtyOutput<'a> {
    Data(&'a [u8]),
    PromptStart,
    PromptEnd,
    PromptContinue,
}

pub struct TerminalExtractor<R: futures::io::AsyncReadExt> {
    buffer: vmap::io::Ring,
    state: State,
    reader: R,
}

pub const START_PROMPT: &str = "\u{200b}\u{200b}";
pub const START_PROMPT_BYTES: &[u8] = START_PROMPT.as_bytes();
pub const END_PROMPT: &str = "\u{200e}\u{200e}";
pub const END_PROMPT_BYTES: &[u8] = END_PROMPT.as_bytes();

impl<R: futures::io::AsyncReadExt + Unpin> TerminalExtractor<R> {
    pub fn new(reader: R) -> Result<Self> {
        Ok(Self {
            buffer: Ring::new(1024 * 16)?,
            state: State::Read,
            reader,
        })
    }

    #[instrument(skip_all)]
    pub async fn next<'a>(&'a mut self) -> Result<PtyOutput<'a>> {
        loop {
            trace!(?self.state, read_len = ?self.buffer.read_len());
            match self.state {
                State::Read => {
                    let buf = &mut self.buffer;

                    let buffered = {
                        let slice = buf.as_write_slice(usize::MAX);
                        self.reader.read(slice).await?
                    };
                    buf.feed(buffered);
                    self.state = State::Process;
                }
                State::Process => {
                    let data = self.buffer.as_read_slice(usize::MAX);

                    let start_prompt_pos = memmem::find(data, START_PROMPT_BYTES);
                    let end_prompt_pos = memmem::find(data, END_PROMPT_BYTES);

                    trace!(?start_prompt_pos, ?end_prompt_pos);

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
                            PtyOutput::Data(&data[..pos])
                        }
                        (Some(start), Some(end)) => {
                            let valid_data_len = cmp::min(start, end);
                            self.state = State::Consume(valid_data_len);
                            PtyOutput::Data(&data[..valid_data_len])
                        }
                        (None, None) => {
                            let data_len = data.len();
                            let num_partial_start_bytes_match =
                                (1..cmp::min(START_PROMPT_BYTES.len(), data_len))
                                    .rev()
                                    .find(|i| START_PROMPT_BYTES.starts_with(&data[data_len - i..]))
                                    .unwrap_or_default();
                            let num_partial_end_bytes_match =
                                (1..cmp::min(END_PROMPT_BYTES.len(), data_len))
                                    .rev()
                                    .find(|i| START_PROMPT_BYTES.starts_with(&data[data_len - i..]))
                                    .unwrap_or_default();

                            let max_bytes = cmp::max(
                                num_partial_start_bytes_match,
                                num_partial_end_bytes_match,
                            );

                            let valid_data_len = data_len - max_bytes;
                            self.state = State::Consume(valid_data_len);
                            PtyOutput::Data(&data[..valid_data_len])
                        }
                    });
                }
                State::Consume(len) => {
                    let buf = &mut self.buffer;
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
    use super::*;
    use async_compat::CompatExt;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_basic_extraction() {
        let mut extractor = TerminalExtractor::new(
            "some initial output here\u{200b}\u{200b}My fancy prompt>\u{200e}\u{200e}".as_bytes(),
        )
        .unwrap();

        assert_eq!(
            extractor.next().await.unwrap(),
            PtyOutput::Data("some initial output here".as_bytes()),
        );
        assert_eq!(extractor.next().await.unwrap(), PtyOutput::PromptStart);
        assert_eq!(
            extractor.next().await.unwrap(),
            PtyOutput::Data("My fancy prompt>".as_bytes()),
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

        let mut extractor = TerminalExtractor::new(client.compat()).unwrap();

        assert_eq!(
            extractor.next().await.unwrap(),
            PtyOutput::Data("some initial output here".as_bytes())
        );

        server
            .write_all("\u{200b}My fancy prompt>\u{200e}\u{200e}".as_bytes())
            .await
            .unwrap();

        assert_eq!(extractor.next().await.unwrap(), PtyOutput::PromptStart);
        assert_eq!(
            extractor.next().await.unwrap(),
            PtyOutput::Data("My fancy prompt>".as_bytes()),
        );
        assert_eq!(extractor.next().await.unwrap(), PtyOutput::PromptEnd);
    }
}