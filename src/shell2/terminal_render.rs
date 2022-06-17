use super::pty_terminal::PtyOutput;
use anyhow::Result;
use tokio::io::AsyncWriteExt;

struct TerminalRender<W: AsyncWriteExt> {
    stdout: W,
}

impl<W: AsyncWriteExt + Unpin> TerminalRender<W> {
    pub async fn handle_pty_output<'a>(&mut self, output: &'a PtyOutput<'a>) -> Result<()> {
        match output {
            PtyOutput::Data(data) => {
                self.stdout.write_all(data).await?;
                self.stdout.flush().await?;
            }
            PtyOutput::PromptStart => {
                self.stdout
                    .write_all("\u{001b}[31m[REC]\u{001b}[0m".as_bytes())
                    .await?;
            }
            _ => {}
        }
        Ok(())
    }
}
