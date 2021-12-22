use crate::config::api_client_configuration;
use anyhow::{Context, Error, Result};
use clap::{ArgEnum, Parser};
use fiberplane_api::apis::default_api::{
    trigger_create, trigger_delete, trigger_get, trigger_list, trigger_webhook,
};
use fiberplane_api::models::NewTrigger;
use std::path::PathBuf;
use std::str::FromStr;
use tokio::fs;
use url::Url;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.subcmd {
        Create(args) => handle_trigger_create_command(args).await,
    }
}

#[derive(Parser)]
pub enum SubCommand {
    #[clap(name = "create", alias = "new", about = "Create a Trigger")]
    Create(CreateArguments),
}

#[derive(Parser)]
pub struct CreateArguments {
    #[clap(name = "template", about = "URL or path to template file")]
    template_source: TemplateSource,

    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,
}

#[derive(ArgEnum)]
enum TemplateSource {
    #[clap(about = "Template URL")]
    Url(Url),
    #[clap(about = "Path to template file")]
    Path(PathBuf),
}

impl FromStr for TemplateSource {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(url) = Url::parse(s) {
            if !url.cannot_be_a_base() {
                return Ok(TemplateSource::Url(url));
            }
        }
        Ok(TemplateSource::Path(PathBuf::from(s)))
    }
}

async fn handle_trigger_create_command(args: CreateArguments) -> Result<()> {
    let config = api_client_configuration(args.config.as_deref(), &args.base_url).await?;
    let new_trigger = match args.template_source {
        TemplateSource::Path(path) => {
            let template_body = fs::read_to_string(&path)
                .await
                .with_context(|| format!("Error reading template from file: {}", path.display()))?;
            NewTrigger {
                template_body: Some(template_body),
                template_url: None,
            }
        }
        TemplateSource::Url(template_url) => NewTrigger {
            template_body: None,
            template_url: Some(template_url.to_string()),
        },
    };
    let trigger = trigger_create(&config, Some(new_trigger))
        .await
        .with_context(|| "Error creating trigger")?;

    eprintln!(
        "Created trigger: {}/api/triggers/{}",
        args.base_url, trigger.id
    );
    eprintln!(
        "Trigger can be invoked with an HTTP POST to: {}/api/triggers/{}/webhook",
        args.base_url, trigger.id
    );
    Ok(())
}
