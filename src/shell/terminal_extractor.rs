use anyhow::Result;
use memchr::memmem::Finder;
use once_cell::sync::OnceCell;
use std::{cmp, fmt::Debug, io::BufRead};
use tracing::{instrument, trace};
use vmap::io::{Ring, SeqRead, SeqWrite};

#[derive(Debug, PartialEq, Eq)]
enum State {
    Process,
    Read,
    Consume(usize),
}

#[derive(PartialEq)]
pub enum PtyOutput<'a> {
    Data(&'a [u8]),
    PromptStart,
    PromptEnd,
}

impl Debug for PtyOutput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Data(arg0) => f
                .debug_tuple("Data")
                .field(&arg0.len() as &dyn Debug)
                .finish(),
            Self::PromptStart => write!(f, "PromptStart"),
            Self::PromptEnd => write!(f, "PromptEnd"),
        }
    }
}

pub struct TerminalExtractor<R: tokio::io::AsyncReadExt> {
    buffer: vmap::io::Ring,
    state: State,
    reader: R,
}

// ---------------------- ATTENTION ----------------------
// Don't change these unless you know what you're doing!
// Due to the way powershell works ($deity knows why)
// the *CHARACTERS* we remove from the stdout stream of the
// pty MUST be equal to the number of *CHARACTERS* we insert
// in order to print the [REC] part of the terminal window.
// In short:
// `assert_eq!(START_PROMPT_REPEATS + END_PROMPT_REPEATS, "[REC]".len())`
//
// More details:
// When a user types in a string like "pwd" powershell outputs
// the following ansi stuff on my system:
// CSI(Mode(ResetDecPrivateMode(Code(ShowCursor))
// CSI(Sgr(Foreground(PaletteIndex(11))))
// Print('p')  <--------------------------------------------------------------------------- (1)
// CSI(Mode(SetDecPrivateMode(Code(ShowCursor))))
// CSI(Sgr(Reset))
// CSI(Mode(ResetDecPrivateMode(Code(ShowCursor))))
// CSI(Sgr(Foreground(PaletteIndex(11))))
// Control(Backspace)  <------------------------------------------------------------------- (2)
// Print('p')
// Print('w')
// CSI(Mode(SetDecPrivateMode(Code(ShowCursor))))
// CSI(Sgr(Reset))
// CSI(Mode(ResetDecPrivateMode(Code(ShowCursor))))
// CSI(Sgr(Foreground(PaletteIndex(11))))
// CSI(Cursor(Position { line: OneBased { value: 2 }, col: OneBased { value: 44 } }))  <--- (3)
// Print('p')
// Print('w')
// Print('d')
// CSI(Mode(SetDecPrivateMode(Code(ShowCursor))))
//
// As you can see that's all over place...
// First character (1) it just prints 'p'
// Second character (2) it first erases 'p' and then
// prints 'p' and 'w'???
// Third character (3) it realizes that's dumb and instead
// SETS the cursor position to the position of 'p'
// and then prints out all 3 characters again???
// What can go wrong here is that the pty we're recording
// is completely unaware that we're fiddling with the output
// and as such it sets the cursor to the position it thinks
// the end of the prompt is at which is different IF
// the number of bytes we replace in the prompt don't line up
pub const START_PROMPT_CHAR: char = '\u{200b}';
pub const START_PROMPT: &str = "\u{200b}\u{200b}\u{200b}";
pub const START_PROMPT_BYTES: &[u8] = START_PROMPT.as_bytes();
pub const START_PROMPT_REPEATS: usize = START_PROMPT_BYTES.len() / START_PROMPT_CHAR.len_utf8();
pub const END_PROMPT_CHAR: char = '\u{200c}';
pub const END_PROMPT: &str = "\u{200c}\u{200c}";
pub const END_PROMPT_BYTES: &[u8] = END_PROMPT.as_bytes();
pub const END_PROMPT_REPEATS: usize = END_PROMPT_BYTES.len() / END_PROMPT_CHAR.len_utf8();

fn start_prompt_finder() -> &'static Finder<'static> {
    static START_PROMPT_FINDER: OnceCell<Finder> = OnceCell::new();
    START_PROMPT_FINDER.get_or_init(|| Finder::new(START_PROMPT_BYTES))
}
fn end_prompt_finder() -> &'static Finder<'static> {
    static END_PROMPT_FINDER: OnceCell<Finder> = OnceCell::new();
    END_PROMPT_FINDER.get_or_init(|| Finder::new(END_PROMPT_BYTES))
}

#[inline]
fn partially_matching_needle_len(data: &[u8], needle: &[u8]) -> usize {
    let data_len = data.len();
    (0..=cmp::min(needle.len(), data_len))
        .rev()
        .find(|i| {
            let start = data_len - i;
            needle.starts_with(&data[start..])
        })
        .unwrap_or_default()
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
            trace!(
                avail = self.buffer.read_len(),
                remain = self.buffer.write_len()
            );
            match self.state {
                State::Read => {
                    let slice = self.buffer.as_write_slice(usize::MAX);
                    let read = self.reader.read(slice).await?;
                    self.buffer.feed(read);
                    trace!(?self.state, read);
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
                            PtyOutput::Data(self.buffer.as_read_slice(pos))
                        }
                        // In the case where we found both markers we should only consume data until the first one
                        // that was found
                        (Some(start), Some(end)) => {
                            let valid_data_len = cmp::min(start, end);
                            self.state = State::Consume(valid_data_len);
                            PtyOutput::Data(self.buffer.as_read_slice(valid_data_len))
                        }
                        // The last case is where we didn't find any *full* markers.
                        (None, None) => {
                            let data = self.buffer.as_read_slice(usize::MAX);
                            // Unfortunately there's no guarantee that we actually read all of a marker
                            // since a full marker is 6 bytes long.
                            // As such we need to check if the *last* 5,4,3,2 and 1 bytes of the read bytes
                            // contains *first* 5,4,3,2 or 1 bytes of either marker.
                            let max_bytes = cmp::max(
                                partially_matching_needle_len(data, START_PROMPT_BYTES),
                                partially_matching_needle_len(data, END_PROMPT_BYTES),
                            );

                            // We subtract the max number of matched bytes from the available data length
                            // and consume that much. Then next round trip we can read more data and check
                            // if the partial matching bytes fully match the marker bytes
                            let valid_data_len = data.len() - max_bytes;
                            if valid_data_len == 0 {
                                self.state = State::Read;
                                continue;
                            } else {
                                self.state = State::Consume(valid_data_len);
                                PtyOutput::Data(self.buffer.as_read_slice(valid_data_len))
                            }
                        }
                    });
                }
                State::Consume(len) => {
                    trace!(?self.state);
                    self.buffer.consume(len);
                    self.state = if self.buffer.read_len()
                        >= cmp::min(START_PROMPT_BYTES.len(), END_PROMPT_BYTES.len())
                    {
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

    #[test]
    fn test_partial_needle() {
        assert_eq!(
            partially_matching_needle_len("data".as_bytes(), START_PROMPT_BYTES),
            0
        );
        for i in 0..=START_PROMPT_BYTES.len() {
            assert_eq!(
                partially_matching_needle_len(&START_PROMPT_BYTES[0..i], START_PROMPT_BYTES),
                i
            );
        }
    }

    #[tokio::test]
    async fn test_basic_extraction() {
        let mut extractor = TerminalExtractor::new(
            "some initial output here\u{200b}\u{200b}\u{200b}My fancy prompt>\u{200c}\u{200c}"
                .as_bytes(),
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
            .write_all("some initial output here\u{200b}\u{200b}".as_bytes())
            .await
            .unwrap();

        let mut extractor = TerminalExtractor::new(client).unwrap();

        assert_eq!(
            extractor.next().await.unwrap(),
            PtyOutput::Data("some initial output here".as_bytes())
        );

        server
            .write_all("\u{200b}My fancy prompt>\u{200c}\u{200c}".as_bytes())
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
