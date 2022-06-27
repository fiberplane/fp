use super::terminal_extractor::PtyOutput;
use anyhow::Result;
use crossterm::style::{Color, Stylize};
use once_cell::sync::OnceCell;
use std::io::Write;
use tokio::io::AsyncWriteExt;

pub struct TerminalRenderer<W: AsyncWriteExt> {
    stdout: W,
}

fn get_styled_bytes() -> &'static [u8] {
    static STYLED_BYTES: OnceCell<Vec<u8>> = OnceCell::new();
    STYLED_BYTES.get_or_init(|| {
        let mut buf = Vec::new();
        let styled = "[REC]".with(Color::Red);
        //this produces something along the lines of this terminal escape output:
        //\u{001b}[31m[REC]\u{001b}[0m
        write!(&mut buf, "{}", styled).unwrap();
        buf
    })
}

impl<W: AsyncWriteExt + Unpin> TerminalRenderer<W> {
    pub fn new(stdout: W) -> Self {
        Self { stdout }
    }
    pub async fn handle_pty_output<'a>(&mut self, output: &'a PtyOutput<'a>) -> Result<()> {
        match output {
            PtyOutput::Data(data) => {
                self.stdout.write_all(data).await?;
                self.stdout.flush().await?;
            }
            PtyOutput::PromptStart => {
                self.stdout.write_all(get_styled_bytes()).await?;
            }
            _ => {}
        }
        Ok(())
    }
}
