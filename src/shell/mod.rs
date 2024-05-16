use self::notebook_writer::NotebookWriter;
use self::pty_terminal::PtyTerminal;
use self::shell_launcher::{ShellLauncher, NESTED_SHELL_SESSION_ENV_VAR_NAME};
use self::terminal_extractor::{PtyOutput, TerminalExtractor};
use self::terminal_renderer::TerminalRenderer;
use self::text_renderer::TextRenderer;
use crate::config::api_client_configuration;
use crate::interactive;
use anyhow::Result;
use clap::Parser;
use fiberplane::base64uuid::Base64Uuid;
use std::time::Duration;
use tracing::instrument;

mod notebook_writer;
mod pty_terminal;
mod shell_launcher;
pub mod shell_type;
mod terminal_extractor;
mod terminal_renderer;
mod text_renderer;

#[derive(Parser)]
pub struct Arguments {
    // ID of the notebook
    #[clap(long, short, env)]
    notebook_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Option<url::Url>,

    #[clap(from_global)]
    profile: Option<String>,

    #[clap(from_global)]
    token: Option<String>,
}

const TEXT_BUF_SIZE: usize = 256;

#[instrument(err, skip_all)]
pub(crate) async fn handle_command(args: Arguments) -> Result<()> {
    if std::env::var(NESTED_SHELL_SESSION_ENV_VAR_NAME).is_ok() {
        return Err(anyhow::anyhow!(
            "Can't start recording inside an existing recording session"
        ));
    }

    let client = api_client_configuration(args.token, args.profile, args.base_url).await?;
    let notebook_id = interactive::notebook_picker(&client, args.notebook_id, None).await?;

    let launcher = ShellLauncher::new(notebook_id.into());
    let mut term_renderer = TerminalRenderer::new(tokio::io::stdout());
    let mut initialized = false;
    let mut interval = tokio::time::interval(Duration::from_millis(250));

    let (notebook_writer, (mut terminal, pty_reader)) = tokio::try_join!(
        NotebookWriter::new(client, notebook_id),
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
