use super::{parser_iter::ParserIter, terminal_extractor::PtyOutput};
use anyhow::Result;
use termwiz::escape::{
    csi::{DecPrivateMode, DecPrivateModeCode, Mode},
    parser::Parser,
    Action, ControlCode, CSI,
};
use tokio::io::AsyncWriteExt;

pub struct TextRender<W: AsyncWriteExt> {
    parser: Parser,
    alternate_mode: bool,
    writer: W,
    current_line: String,
    position: usize,
}

impl<W: AsyncWriteExt + Unpin> TextRender<W> {
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

    pub async fn flush(
        writer: &mut W,
        current_line: &mut String,
        position: &mut usize,
    ) -> Result<()> {
        writer.write_all(current_line.as_bytes()).await?;
        current_line.clear();
        *position = 0;

        Ok(())
    }

    pub async fn handle_pty_output<'a>(&mut self, output: &'a PtyOutput<'a>) -> Result<()> {
        match output {
            PtyOutput::Data(data) => self.on_data(data).await?,
            _ => {}
        }
        Ok(())
    }

    pub async fn on_data(&mut self, data: &[u8]) -> Result<()> {
        let parser = &mut self.parser;

        for action in ParserIter::new(parser, data) {
            match action {
                Action::Print(c) => {
                    if !self.alternate_mode {
                        self.current_line.push(c);
                    }
                }
                Action::Control(ControlCode::LineFeed) => {
                    if !self.alternate_mode {
                        self.current_line.push('\n');
                        Self::flush(&mut self.writer, &mut self.current_line, &mut self.position)
                            .await?;
                    }
                }
                Action::Control(ControlCode::Backspace) => {
                    if !self.alternate_mode {
                        self.current_line.remove(self.position);
                    }
                }
                Action::CSI(CSI::Mode(Mode::SetDecPrivateMode(DecPrivateMode::Code(code)))) => {
                    match code {
                        DecPrivateModeCode::ClearAndEnableAlternateScreen => {
                            self.alternate_mode = true
                        }
                        DecPrivateModeCode::EnableAlternateScreen => self.alternate_mode = true,
                        DecPrivateModeCode::OptEnableAlternateScreen => self.alternate_mode = true,
                        _ => {}
                    }
                }
                Action::CSI(CSI::Mode(Mode::ResetDecPrivateMode(DecPrivateMode::Code(code)))) => {
                    match code {
                        DecPrivateModeCode::ClearAndEnableAlternateScreen => {
                            self.alternate_mode = false
                        }
                        DecPrivateModeCode::EnableAlternateScreen => self.alternate_mode = false,
                        DecPrivateModeCode::OptEnableAlternateScreen => self.alternate_mode = false,
                        _ => {}
                    }
                }
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
        let mut render = TextRender::new(&mut buf);
        render.on_data("hello world\n".as_bytes()).await.unwrap();
        assert_eq!(&buf, "hello world\n".as_bytes());
    }

    #[tokio::test]
    async fn strips_alternate_mode() {
        let mut buf = vec![];
        let mut render = TextRender::new(&mut buf);
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
