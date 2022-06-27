mod notebook_writer;
mod pty_terminal;
mod shell_launcher;
mod shell_type;
mod terminal_extractor;
mod terminal_render;
mod text_render;

use self::{
    notebook_writer::NotebookWriter,
    pty_terminal::PtyTerminal,
    shell_launcher::ShellLauncher,
    terminal_extractor::{PtyOutput, TerminalExtractor},
    terminal_render::TerminalRender,
    text_render::TextRender,
};
use crate::config::api_client_configuration;
use anyhow::Result;
use clap::Parser;
use std::{path::PathBuf, time::Duration};
use tracing::instrument;

#[derive(Parser)]
pub struct Arguments {
    // ID of the notebook
    #[clap(name = "id", env = "NOTEBOOK_ID")]
    id: String,

    #[clap(parse(from_flag), env = "__FP_SHELL_SESSION")]
    nested: bool,

    #[clap(from_global)]
    base_url: url::Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

const TEXT_BUF_SIZE: usize = 256;

#[instrument(err, skip_all)]
pub(crate) async fn handle_command(args: Arguments) -> Result<()> {
    if args.nested {
        eprintln!("Can't start recording inside an existing recording session");
        return Ok(());
    }

    let config = api_client_configuration(args.config, &args.base_url).await?;
    let launcher = ShellLauncher::new(args.id.clone());
    let mut term_render = TerminalRender::new(tokio::io::stdout());
    let mut initialized = false;
    let mut interval = tokio::time::interval(Duration::from_millis(250));

    let (notebook_writer, (mut terminal, pty_reader)) = tokio::try_join!(
        NotebookWriter::new(config, args.id),
        PtyTerminal::new(launcher)
    )?;

    let mut term_extractor = TerminalExtractor::new(pty_reader)?;
    let mut text_render = TextRender::new(Vec::with_capacity(TEXT_BUF_SIZE));

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
                    term_render.handle_pty_output(&output),
                    text_render.handle_pty_output(&output)
                )?;
            }
            _ = interval.tick() => {
                let inner = text_render.inner_mut();
                if inner.is_empty() {
                    continue;
                }

                let buffer = std::mem::replace(inner, Vec::with_capacity(TEXT_BUF_SIZE));
                notebook_writer.write(buffer).await?;
            }
        }
    }

    text_render.flush().await?;

    notebook_writer
        .write(std::mem::replace(text_render.inner_mut(), Vec::new()))
        .await?;
    notebook_writer.close().await?;

    Ok(())
}