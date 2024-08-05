use super::terminal_extractor::PtyOutput;
use anyhow::Result;
use termwiz::escape::csi::{DecPrivateMode, DecPrivateModeCode, Mode};
use termwiz::escape::{parser::Parser, Action, ControlCode, CSI};
use tokio::io::AsyncWriteExt;
use tracing::trace;

pub struct TextRenderer<W: AsyncWriteExt> {
    parser: Parser,
    alternate_mode: bool,
    writer: W,
    current_line: String,
    position: usize,
}

impl<W: AsyncWriteExt + Unpin> TextRenderer<W> {
    pub fn new(writer: W) -> Self {
        Self {
            parser: Parser::new(),
            alternate_mode: false,
            writer,
            current_line: String::new(),
            position: 0,
        }
    }

    pub fn inner_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    fn push_char(&mut self, c: char) {
        if !self.alternate_mode {
            self.current_line.push(c);
            self.position += c.len_utf8();
        }
    }

    pub async fn flush(&mut self) -> Result<()> {
        self.writer.write_all(self.current_line.as_bytes()).await?;
        self.current_line.clear();
        self.position = 0;

        Ok(())
    }

    pub async fn handle_pty_output<'a>(&mut self, output: &'a PtyOutput<'a>) -> Result<()> {
        if let PtyOutput::Data(data) = output {
            self.on_data(data).await?
        }
        Ok(())
    }

    pub async fn on_data(&mut self, data: &[u8]) -> Result<()> {
        let mut i = 0;
        while let Some((action, consumed)) = self.parser.parse_first(&data[i..]) {
            i += consumed;
            trace!(?action);
            match action {
                Action::Print(c) => self.push_char(c),
                Action::Control(c @ ControlCode::HorizontalTab) => self.push_char(c as u8 as char),
                Action::Control(c @ ControlCode::LineFeed) => {
                    self.push_char(c as u8 as char);
                    self.flush().await?;
                }
                Action::Control(ControlCode::Backspace) => {
                    if !self.alternate_mode {
                        trace!(
                            "Removing char at position {}, from current_line with len {}",
                            self.position,
                            self.current_line.len()
                        );
                        let len = if self.position == self.current_line.len() {
                            self.current_line
                                .pop()
                                .map(char::len_utf8)
                                .unwrap_or_default()
                        } else {
                            self.current_line.remove(self.position).len_utf8()
                        };

                        self.position -= len;
                    }
                }
                // This matches the magic incantation terminal programs output in order to enter alternate mode/screen:
                // https://superuser.com/a/321233
                // Since that mode is generally used for interactive programs like htop and vim we don't want
                // to render them to text since that'd look really weird
                Action::CSI(CSI::Mode(Mode::SetDecPrivateMode(DecPrivateMode::Code(
                    DecPrivateModeCode::ClearAndEnableAlternateScreen
                    | DecPrivateModeCode::EnableAlternateScreen
                    | DecPrivateModeCode::OptEnableAlternateScreen,
                )))) => self.alternate_mode = true,

                Action::CSI(CSI::Mode(Mode::ResetDecPrivateMode(DecPrivateMode::Code(
                    DecPrivateModeCode::ClearAndEnableAlternateScreen
                    | DecPrivateModeCode::EnableAlternateScreen
                    | DecPrivateModeCode::OptEnableAlternateScreen,
                )))) => self.alternate_mode = false,
                _ => {}
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn basic_test() {
        let mut buf = vec![];
        let mut render = TextRenderer::new(&mut buf);
        render.on_data("hello world\n".as_bytes()).await.unwrap();
        assert_eq!(&buf, "hello world\n".as_bytes());
    }

    #[tokio::test]
    async fn strips_alternate_mode() {
        let mut buf = vec![];
        let mut render = TextRenderer::new(&mut buf);
        render
            .on_data(
                format!(
                    "hello{}you shouldn't see this\n{} world\n",
                    Action::CSI(CSI::Mode(Mode::SetDecPrivateMode(DecPrivateMode::Code(
                        DecPrivateModeCode::ClearAndEnableAlternateScreen
                    )))),
                    Action::CSI(CSI::Mode(Mode::ResetDecPrivateMode(DecPrivateMode::Code(
                        DecPrivateModeCode::ClearAndEnableAlternateScreen
                    ))))
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        assert_eq!(&buf, "hello world\n".as_bytes());
    }
}
