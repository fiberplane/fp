use anyhow::{anyhow, Result};
use clap::Parser;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.subcmd {
        Trigger(args) => handle_trigger_command(args).await,
    }
}

#[derive(Parser)]
pub enum SubCommand {
    #[clap(name = "trigger", about = "Monitor a fiberplane realtime connection")]
    Trigger(TriggerArguments),
}

#[derive(Parser)]
pub struct TriggerArguments {
    #[clap(name = "labels", long, short, about = "Sets the alert labels")]
    pub labels: Vec<String>,

    #[clap(name = "annotations", long, short, about = "Set the alert annotations")]
    pub annotations: Vec<String>,
}

async fn handle_trigger_command(args: TriggerArguments) -> Result<()> {
    let mut labels: HashMap<String, String> = HashMap::new();

    for l in args.labels {
        let vec: Vec<&str> = l.split('=').collect();
        labels.insert(vec[0].to_string(), vec[1].to_string());
    }

    let wht = WebhookTrigger {
        id: "amazing webhook id".to_string(),
        labels,
    };

    do_request(wht)
        .await
        .map_err(|e| anyhow!("request failed: {:?}", e))?;
    println!("trigger!");
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct WebhookTrigger {
    id: String,
    labels: HashMap<String, String>,
}

async fn do_request(wht: WebhookTrigger) -> Result<(), reqwest::Error> {
    let _ = Client::new()
        .post("https://dev.fiberplane.io")
        .json(&wht)
        .send()
        .await?
        .json()
        .await?;

    Ok(())
}
