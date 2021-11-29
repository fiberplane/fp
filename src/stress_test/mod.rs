mod worker;

use std::time::Duration;

use crate::config::api_client_configuration;
use anyhow::{anyhow, Result};
use clap::Parser;
use fiberplane::{
    operations::Notebook,
    protocols::{
        core::{Cell, CheckboxCell, CodeCell, GraphCell, TextCell, TimeRange},
        realtime,
    },
};
use futures_util::{pin_mut, SinkExt, StreamExt};
use tokio::io::AsyncWriteExt;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::debug;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser)]
pub enum SubCommand {
    #[clap(name = "execute", alias = "exe", about = "Execute a stress test")]
    Execute(ExecuteArguments),
}

#[derive(Parser, Clone)]
pub struct ExecuteArguments {
    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,

    #[clap(
        name = "notebook_id",
        long,
        short,
        about = "id of the notebook to spam"
    )]
    notebook_id: String,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.subcmd {
        SubCommand::Execute(exe) => execute_stress_test(exe).await,
    }
}

pub async fn execute_stress_test(args: ExecuteArguments) -> Result<()> {
    let worker = worker::Worker::new(args.base_url, args.notebook_id, args.config).await?;

    worker.insert_text_cell("Hello world?".to_owned()).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    worker.insert_text_cell("Hello foobar!".to_owned()).await;

    tokio::time::sleep(Duration::from_secs(10)).await;

    Ok(())
}
