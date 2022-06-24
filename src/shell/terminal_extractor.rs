use anyhow::Result;
use memchr::memmem::Finder;
use once_cell::sync::OnceCell;
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
}

pub struct TerminalExtractor<R: tokio::io::AsyncReadExt> {
    buffer: vmap::io::Ring,
    state: State,
    reader: R,
}

pub const START_PROMPT_CHAR: char = '\u{200b}';
pub const START_PROMPT: &str = "\u{200b}\u{200b}";
pub const START_PROMPT_BYTES: &[u8] = START_PROMPT.as_bytes();
pub const END_PROMPT_CHAR: char = '\u{200e}';
pub const END_PROMPT: &str = "\u{200e}\u{200e}";
pub const END_PROMPT_BYTES: &[u8] = END_PROMPT.as_bytes();

fn start_prompt_finder() -> &'static Finder<'static> {
    static START_PROMPT_FINDER: OnceCell<Finder> = OnceCell::new();
    START_PROMPT_FINDER.get_or_init(|| Finder::new(START_PROMPT_BYTES))
}
fn end_prompt_finder() -> &'static Finder<'static> {
    static END_PROMPT_FINDER: OnceCell<Finder> = OnceCell::new();
    END_PROMPT_FINDER.get_or_init(|| Finder::new(END_PROMPT_BYTES))
}

impl<R: tokio::io::AsyncReadExt + Unpin> TerminalExtractor<R> {
    pub fn new(reader: R) -> Result<Self> {
        Ok(Self {
            buffer: Ring::new(1024 * 16)?,
            state: State::Read,
            reader,
        })
    }

    #[instrument(skip_all, ret)]
    pub async fn next<'a>(&'a mut self) -> Result<PtyOutput<'a>> {
        loop {
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

                    let start_prompt_pos = start_prompt_finder().find(data);
                    let end_prompt_pos = end_prompt_finder().find(data);

                    trace!(?start_prompt_pos, ?end_prompt_pos);

                    // This is the main work horse of the terminal extractor loop.
                    // Basically it searches for our START and END markers of the terminal prompt in
                    // the byte stream we read in.
                    return Ok(match (start_prompt_pos, end_prompt_pos) {
                        // These first 2 cases where we've found a START or END marker at the very beginning
                        // of the read buffer we simply output those markers and set the next state to consume
                        // the full length of the markers.
                        (Some(0), _) => {
                            self.state = State::Consume(START_PROMPT_BYTES.len());
                            PtyOutput::PromptStart
                        }
                        (_, Some(0)) => {
                            self.state = State::Consume(END_PROMPT_BYTES.len());
                            PtyOutput::PromptEnd
                        }
                        // In the case where only one marker at non zero index was found we output and consume
                        // the data up to the marker.
                        // It should be noted that this approach means multiple `PtyOutput::Data()` may be output
                        // in a row.
                        (Some(pos), None) | (None, Some(pos)) => {
                            self.state = State::Consume(pos);
                            PtyOutput::Data(&data[..pos])
                        }
                        // In the case where we found both markers we should only consume data until the first one
                        // that was found
                        (Some(start), Some(end)) => {
                            let valid_data_len = cmp::min(start, end);
                            self.state = State::Consume(valid_data_len);
                            PtyOutput::Data(&data[..valid_data_len])
                        }
                        // The last case is where we didn't find any *full* markers.
                        (None, None) => {
                            // Unfortunately there's no guarantee that we actually read all of a marker
                            // since a full marker is 6 bytes long.
                            // As such we need to check if the *last* 5,4,3,2 and 1 bytes of the read bytes
                            // contains *first* 5,4,3,2 or 1 bytes of either marker.
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

                            // We subtract the max number of matched bytes from the available data length
                            // and consume that much. Then next round trip we can read more data and check
                            // if the partial matching bytes fully match the marker bytes
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

        let mut extractor = TerminalExtractor::new(client).unwrap();

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
