use self::cell_writer::CellWriter;
use crate::output::{output_details, output_json, GenericKeyValue};
use crate::shell::shell_type::ShellType;
use crate::{config::api_client_configuration, fp_urls::NotebookUrlBuilder, interactive};
use anyhow::Result;
use base64uuid::Base64Uuid;
use clap::{Parser, ValueEnum, ValueHint};
use fp_api_client::models::Cell;
use futures::StreamExt;
use std::io::ErrorKind;
use std::{env, path::PathBuf, process::Stdio};
use tokio::io::{self, AsyncWriteExt};
use tokio::{process::Command, signal};
use tokio_util::io::ReaderStream;
use tracing::{debug, info};
use url::Url;

pub mod cell_writer;
mod parse_logs;
mod timestamp;

#[derive(Parser, Clone)]
pub struct Arguments {
    /// The notebook to append the message to
    #[clap(long, short, env)]
    notebook_id: Option<Base64Uuid>,

    /// The command to run
    #[clap(value_hint = ValueHint::CommandWithArguments, num_args = 1..)]
    command: Vec<String>,

    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    /// Output type to display
    #[clap(long, short, default_value = "command", value_enum)]
    output: ExecOutput,
}

#[derive(ValueEnum, Clone, PartialEq)]
enum ExecOutput {
    /// Output the result of the command
    Command,

    /// Output the cell details as a table
    Table,

    /// Output the cell details as a JSON encoded object
    Json,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    let config = api_client_configuration(args.config.clone(), &args.base_url).await?;
    let command = args.command.join(" ");

    let workspace_id = interactive::workspace_picker(&config, args.workspace_id).await?;
    let notebook_id =
        interactive::notebook_picker(&config, args.notebook_id, Some(workspace_id)).await?;

    let (shell_type, shell_path) = ShellType::auto_detect();
    debug!("Using {:?} to run command: \"{}\"", shell_type, &command);

    let mut child = Command::new(shell_path)
        .arg("-c")
        .arg(command)
        .current_dir(env::current_dir()?)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|err| {
            if err.kind() == ErrorKind::NotFound {
                anyhow::anyhow!("Command not found: {}", args.command[0])
            } else {
                anyhow::anyhow!("Failed to run command: {}", err)
            }
        })?;

    let mut child_stdout = ReaderStream::new(child.stdout.take().unwrap());
    let mut child_stderr = ReaderStream::new(child.stderr.take().unwrap());
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();

    let mut cell_writer = CellWriter::new(config, notebook_id, args.command);

    loop {
        tokio::select! {
            biased;
            // This sets up a ctrl-c handler so that the output will be written even if the process is killed
            // (This is important when using this command with a long-running command that needs to be
            // exited manually but where you still want to see the ouput)
            _ = signal::ctrl_c() => {
                break;
            }
            _ = child.wait() => {
                break;
            }
            chunk = child_stdout.next() => {
                if let Some(Ok(chunk)) = chunk {
                    if args.output == ExecOutput::Command {
                        stdout.write_all(&chunk).await?;
                    }
                    cell_writer.append(chunk);
                }
            }
            chunk = child_stderr.next() => {
                if let Some(Ok(chunk)) = chunk {
                    if args.output == ExecOutput::Command {
                        stderr.write_all(&chunk).await?;
                    }
                    cell_writer.append(chunk);
                }
            }
        }
    }

    cell_writer.flush().await?;

    if let Some(cell) = cell_writer.into_output_cell() {
        let url = NotebookUrlBuilder::new(workspace_id, notebook_id)
            .base_url(args.base_url)
            .cell_id(cell.id())
            .url()?;

        let cell: Cell = serde_json::from_value(serde_json::to_value(cell)?)?;
        match args.output {
            ExecOutput::Command => {
                info!("\n   --> Created cell: {}", url);
                Ok(())
            }
            ExecOutput::Table => {
                info!("Created cell");
                output_details(GenericKeyValue::from_cell(cell))
            }
            ExecOutput::Json => output_json(&cell),
        }
    } else {
        Ok(())
    }
}
