use crate::config::api_client_configuration;
use crate::output::{output_details, output_list, GenericKeyValue};
use crate::templates::TemplateArguments;
use anyhow::{Context, Result};
use base64uuid::Base64Uuid;
use clap::Parser;
use cli_table::Table;
use fp_api_client::apis::configuration::Configuration;
use fp_api_client::apis::default_api::{
    trigger_create, trigger_delete, trigger_get, trigger_invoke, trigger_list,
};
use fp_api_client::models::{NewTrigger, Trigger};
use lazy_static::lazy_static;
use regex::Regex;
use std::path::PathBuf;
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
    /// Name of the trigger
    #[clap(long, alias = "name")]
    title: String,

    /// ID of the template (already uploaded to Fiberplane)
    #[clap(long)]
    template_id: Base64Uuid,

    /// Default arguments to be passed to the template when the trigger is invoked
    /// Can be passed as a JSON object or as a comma-separated list of key=value pairs
    #[clap(long)]
    default_arguments: Option<TemplateArguments>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct IndividualTriggerArguments {
    /// Trigger ID or URL
    #[clap(name = "trigger")]
    trigger: String,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct ListArguments {
    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct InvokeArguments {
    /// Trigger ID or URL
    #[clap()]
    trigger: String,

    /// Secret Key (returned when the trigger is initially created)
    #[clap()]
    secret_key: String,

    /// Values to inject into the template
    /// Can be passed as a JSON object or as a comma-separated list of key=value pairs
    #[clap()]
    template_arguments: Option<TemplateArguments>,

    #[clap(from_global)]
    base_url: Url,
}

async fn handle_trigger_create_command(args: CreateArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let default_arguments = if let Some(default_arguments) = args.default_arguments {
        Some(serde_json::to_value(default_arguments)?)
    } else {
        None
    };
    let trigger = NewTrigger {
        title: args.title,
        default_arguments,
        template_id: args.template_id.to_string(),
    };
    let trigger = trigger_create(&config, trigger)
        .await
        .with_context(|| "Error creating trigger")?;

    let trigger = GenericKeyValue::from_trigger(trigger, args.base_url);

    output_details(trigger)
}

async fn handle_trigger_get_command(args: IndividualTriggerArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let trigger_id = &TRIGGER_ID_REGEX
        .captures(&args.trigger)
        .with_context(|| "Could not parse trigger. Expected a Trigger ID or URL")?[1];
    let trigger = trigger_get(&config, trigger_id)
        .await
        .with_context(|| "Error getting trigger details")?;

    let trigger = GenericKeyValue::from_trigger(trigger, args.base_url);

    output_details(trigger)
}

async fn handle_trigger_delete_command(args: IndividualTriggerArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let trigger_id = &TRIGGER_ID_REGEX
        .captures(&args.trigger)
        .with_context(|| "Could not parse trigger. Expected a Trigger ID or URL")?[1];
    trigger_delete(&config, trigger_id)
        .await
        .with_context(|| "Error deleting trigger")?;

    info!("Deleted trigger");

    Ok(())
}

async fn handle_trigger_list_command(args: ListArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let mut triggers = trigger_list(&config)
        .await
        .with_context(|| "Error getting triggers")?;

    triggers.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    let triggers: Vec<TriggerRow> = triggers.into_iter().map(Into::into).collect();

    output_list(triggers)
}

async fn handle_trigger_invoke_command(args: InvokeArguments) -> Result<()> {
    let trigger_id = &TRIGGER_ID_REGEX
        .captures(&args.trigger)
        .with_context(|| "Could not parse trigger. Expected a Trigger ID or URL")?[1];

    let body = serde_json::to_value(&args.template_arguments)?;

    let config = Configuration {
        base_path: args.base_url.to_string(),
        ..Configuration::default()
    };
    let notebook = trigger_invoke(&config, trigger_id, &args.secret_key, Some(body))
        .await
        .with_context(|| "Error invoking trigger")?;
    info!("Created notebook:");
    println!("{}/notebook/{}", config.base_path, notebook.id);

    Ok(())
}

#[derive(Table)]
struct TriggerRow {
    #[table(title = "Title")]
    title: String,

    #[table(title = "ID")]
    id: String,

    #[table(title = "Template ID")]
    template_id: String,
}

impl From<Trigger> for TriggerRow {
    fn from(trigger: Trigger) -> Self {
        Self {
            title: trigger.title,
            id: trigger.id,
            template_id: trigger.template_id,
        }
    }
}
