mod notebook_writer;
mod pty_terminal;
mod shell_launcher;
mod shell_type;
mod terminal_extractor;
mod terminal_renderer;
mod text_renderer;

use self::{
    notebook_writer::NotebookWriter,
    pty_terminal::PtyTerminal,
    shell_launcher::{ShellLauncher, NESTED_SHELL_SESSION_ENV_VAR_NAME},
    terminal_extractor::{PtyOutput, TerminalExtractor},
    terminal_renderer::TerminalRenderer,
    text_renderer::TextRenderer,
};
use crate::config::api_client_configuration;
use anyhow::Result;
use clap::Parser;
use std::{path::PathBuf, time::Duration};
use tracing::instrument;

#[derive(Parser)]
pub struct Arguments {
    // ID of the notebook
    #[clap()]
    notebook_id: String,

    #[clap(from_global)]
    base_url: url::Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

const TEXT_BUF_SIZE: usize = 256;

#[instrument(err, skip_all)]
pub(crate) async fn handle_command(args: Arguments) -> Result<()> {
    if std::env::var(NESTED_SHELL_SESSION_ENV_VAR_NAME).is_ok() {
        return Err(anyhow::anyhow!(
            "Can't start recording inside an existing recording session"
        ));
    }

    let config = api_client_configuration(args.config, &args.base_url).await?;
    let launcher = ShellLauncher::new(args.notebook_id.clone());
    let mut term_renderer = TerminalRenderer::new(tokio::io::stdout());
    let mut initialized = false;
    let mut interval = tokio::time::interval(Duration::from_millis(250));

    let (notebook_writer, (mut terminal, pty_reader)) = tokio::try_join!(
        NotebookWriter::new(config, args.notebook_id),
        PtyTerminal::new(launcher)
    )?;

    let mut term_extractor = TerminalExtractor::new(pty_reader)?;
    let mut text_renderer = TextRenderer::new(Vec::with_capacity(TEXT_BUF_SIZE));

    // Worker loop that drives the reading of the shell output and forwards it to the
    // terminal and text renders.
    // The text render in turn writes its output to the notebook which internally buffers
    // the text and gets sent to the server on each `flush` on a 250ms interval.
    loop {
        tokio::select! {
            biased;
            _ = terminal.wait_close() => {
                break;
            },
            Ok(output) = term_extractor.next() => {
                if !initialized {
                    // Discard any child output until the terminal is fully initialized
                    // This basically removes the outputting of initialization commands
                    if output != PtyOutput::PromptStart {
                        continue;
                    }

                    initialized = true;
                }

                let _ = tokio::try_join!(
                    term_renderer.handle_pty_output(&output),
                    text_renderer.handle_pty_output(&output)
                )?;
            }
            _ = interval.tick() => {
                let inner = text_renderer.inner_mut();
                if inner.is_empty() {
                    continue;
                }

                let buffer = std::mem::replace(inner, Vec::with_capacity(TEXT_BUF_SIZE));
                notebook_writer.write(buffer).await?;
            }
        }
    }

    text_renderer.flush().await?;

    notebook_writer
        .write(std::mem::take(text_renderer.inner_mut()))
        .await?;
    notebook_writer.close().await?;

    Ok(())
}
