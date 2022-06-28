use self::cell_writer::CellWriter;
use crate::config::api_client_configuration;
use crate::output::{output_details, output_json, GenericKeyValue};
use anyhow::{Context, Result};
use clap::{ArgEnum, Parser};
use fp_api_client::models::Cell;
use futures::StreamExt;
use std::time::Duration;
use std::{path::PathBuf, process::Stdio};
use tokio::io::{self, AsyncWriteExt};
use tokio::{process::Command, signal, time::interval};
use tokio_util::io::ReaderStream;
use tracing::{debug, info};
use url::Url;

pub mod cell_writer;

#[derive(Parser, Clone)]
pub struct Arguments {
    /// The notebook to append the message to
    #[clap(long, short, env)]
    notebook_id: String,

    /// The command to run
    command: String,

    /// Args to pass to the command
    args: Vec<String>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    /// Output type to display
    #[clap(long, short, default_value = "command", arg_enum)]
    output: ExecOutput,
}

#[derive(ArgEnum, Clone, PartialEq)]
enum ExecOutput {
    /// Output the result of the command
    Command,

    /// Output the cell details as a table
    Table,

    /// Output the cell details as a JSON encoded object
    Json,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    debug!("Running command: \"{}\"", args.command);
    let config = api_client_configuration(args.config.clone(), &args.base_url).await?;
    let mut child = Command::new(&args.command)
        .args(&args.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::inherit())
        .spawn()
        .with_context(|| "Error spawning child process to run command")?;

    let mut child_stdout = ReaderStream::new(child.stdout.take().unwrap());
    let mut child_stderr = ReaderStream::new(child.stderr.take().unwrap());
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();

    let mut cell_writer = CellWriter::new(args.clone(), config);

    // Read from the child process' stdout/stderr and write the output to the notebook
    // cell every 250 milliseconds
    let mut send_interval = interval(Duration::from_millis(250));
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
                cell_writer.write_to_cell().await?;
                break;
            }
            _ = send_interval.tick() => {
                cell_writer.write_to_cell().await?;
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

    let cell = cell_writer.into_output_cell();
    let mut url = args
        .base_url
        .join("/notebook/")
        .unwrap()
        .join(&args.notebook_id)
        .unwrap();
    if let Some(cell) = &cell {
        url.set_fragment(Some(cell.id()));
    };

    if let Some(cell) = &cell {
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
