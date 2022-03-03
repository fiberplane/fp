use crate::config::api_client_configuration;
use crate::templates::TemplateArg;
use anyhow::{anyhow, Context, Error, Result};
use clap::{ArgEnum, Parser};
use fp_api_client::apis::configuration::Configuration;
use fp_api_client::apis::default_api::{
    trigger_create, trigger_delete, trigger_get, trigger_invoke, trigger_list,
};
use fp_api_client::models::NewTrigger;
use lazy_static::lazy_static;
use regex::Regex;
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf, str::FromStr};
use tokio::fs;
use tracing::info;
use url::Url;

lazy_static! {
    static ref TRIGGER_ID_REGEX: Regex = Regex::new(r"([a-zA-Z0-9_-]{22})(?:/webhook)?$").unwrap();
}

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Create(args) => handle_trigger_create_command(args).await,
        Get(args) => handle_trigger_get_command(args).await,
        Delete(args) => handle_trigger_delete_command(args).await,
        List(args) => handle_trigger_list_command(args).await,
        Invoke(args) => handle_trigger_invoke_command(args).await,
    }
}

#[derive(Parser)]
enum SubCommand {
    /// Create a Trigger
    #[clap(alias = "new")]
    Create(CreateArguments),
    /// Print info about a trigger
    #[clap(alias = "info")]
    Get(IndividualTriggerArguments),
    /// Delete a trigger
    #[clap(alias = "remove")]
    Delete(IndividualTriggerArguments),
    /// List all triggers
    #[clap()]
    List(ListArguments),
    /// Invoke a trigger webhook to create a notebook from the template
    #[clap()]
    Invoke(InvokeArguments),
}

#[derive(Parser)]
struct CreateArguments {
    /// URL or path to template file
    #[clap(name = "template")]
    template_source: TemplateSource,

    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,
}

#[derive(ArgEnum)]
enum TemplateSource {
    /// Template URL
    #[clap()]
    Url(Url),
    /// Path to template file
    #[clap()]
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

#[derive(Parser)]
struct IndividualTriggerArguments {
    /// Trigger ID or URL
    #[clap(name = "trigger")]
    trigger: String,

    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,
}

#[derive(Parser)]
struct ListArguments {
    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,
}

#[derive(Parser)]
struct InvokeArguments {
    /// Trigger ID or URL
    #[clap()]
    trigger: String,

    /// Values to inject into the template. Must be in the form name=value. JSON values are supported.
    #[clap(name = "arg", short, long)]
    args: Vec<TemplateArg>,

    #[clap(from_global)]
    base_url: String,
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
        TemplateSource::Url(template_url) => {
            if template_url.scheme() != "https" {
                return Err(anyhow!("Template URLs must use HTTPS"));
            }
            NewTrigger {
                template_body: None,
                template_url: Some(template_url.to_string()),
            }
        }
    };
    let trigger = trigger_create(&config, Some(new_trigger))
        .await
        .with_context(|| "Error creating trigger")?;

    info!(
        "Created trigger: {}/api/triggers/{}",
        args.base_url, trigger.id
    );
    info!(
        "Trigger can be invoked with an HTTP POST to: {}/api/triggers/{}/webhook",
        args.base_url, trigger.id
    );
    Ok(())
}

async fn handle_trigger_get_command(args: IndividualTriggerArguments) -> Result<()> {
    let config = api_client_configuration(args.config.as_deref(), &args.base_url).await?;
    let trigger_id = &TRIGGER_ID_REGEX
        .captures(&args.trigger)
        .with_context(|| "Could not parse trigger. Expected a Trigger ID or URL")?[1];
    let trigger = trigger_get(&config, trigger_id)
        .await
        .with_context(|| "Error getting trigger details")?;

    info!("Trigger ID: {}", trigger.id);
    info!(
        "WebHook URL: {}/api/triggers/{}/webhook",
        args.base_url, trigger.id
    );
    info!(
        "Template URL: {}",
        trigger.template_url.as_deref().unwrap_or("N/A")
    );

    // TODO should we default to not printing the body and give an option to print it?
    info!("Template Body:");
    for line in trigger.template_body.lines() {
        info!("  {}", line);
    }

    Ok(())
}

async fn handle_trigger_delete_command(args: IndividualTriggerArguments) -> Result<()> {
    let config = api_client_configuration(args.config.as_deref(), &args.base_url).await?;
    let trigger_id = &TRIGGER_ID_REGEX
        .captures(&args.trigger)
        .with_context(|| "Could not parse trigger. Expected a Trigger ID or URL")?[1];
    trigger_delete(&config, trigger_id)
        .await
        .with_context(|| "Error deleting trigger")?;
    Ok(())
}

async fn handle_trigger_list_command(args: ListArguments) -> Result<()> {
    let config = api_client_configuration(args.config.as_deref(), &args.base_url).await?;
    let mut triggers = trigger_list(&config)
        .await
        .with_context(|| "Error getting triggers")?;

    if triggers.is_empty() {
        info!("(No active triggers found)");
    } else {
        // Show the most recently updated first
        triggers.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        for trigger in triggers {
            info!(
                "- Trigger ID: {id}
  WebHook URL: {base_url}/api/triggers/{id}/webhook
  Template URL: {template_url}",
                id = trigger.id,
                base_url = args.base_url,
                template_url = trigger
                    .template_url
                    .as_deref()
                    .unwrap_or("N/A (Query the individual trigger ID to display stored template)")
            );
        }
    }

    Ok(())
}

async fn handle_trigger_invoke_command(args: InvokeArguments) -> Result<()> {
    let trigger_id = &TRIGGER_ID_REGEX
        .captures(&args.trigger)
        .with_context(|| "Could not parse trigger. Expected a Trigger ID or URL")?[1];
    dbg!(&trigger_id);

    let body: HashMap<String, Value> = args.args.into_iter().map(|a| (a.name, a.value)).collect();
    let body = serde_json::to_value(&body)?;

    let config = Configuration {
        base_path: args.base_url.to_string(),
        ..Configuration::default()
    };
    let result = trigger_invoke(&config, trigger_id, Some(body))
        .await
        .with_context(|| "Error invoking trigger")?;
    info!("Created notebook: {}", result.notebook_url);

    Ok(())
}
