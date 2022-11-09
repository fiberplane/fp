use crate::config::api_client_configuration;
use crate::interactive;
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use crate::templates::TemplateArguments;
use anyhow::{Context, Result};
use base64uuid::Base64Uuid;
use clap::{Parser, ValueEnum};
use cli_table::Table;
use fp_api_client::apis::configuration::Configuration;
use fp_api_client::apis::default_api::{
    trigger_create, trigger_delete, trigger_get, trigger_invoke, trigger_list,
};
use fp_api_client::models::{NewTrigger, Trigger, TriggerInvokeResponse};
use std::path::PathBuf;
use tracing::info;
use url::Url;

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
    /// Create a trigger
    #[clap(alias = "add")]
    Create(CreateArguments),

    /// Retrieve a trigger
    Get(GetArguments),

    /// Delete a trigger
    #[clap(aliases = &["remove", "rm"])]
    Delete(DeleteArguments),

    /// List all triggers
    List(ListArguments),

    /// Invoke a trigger webhook to create a notebook from the template
    Invoke(InvokeArguments),
}

#[derive(Parser)]
struct CreateArguments {
    /// Workspace to create the trigger in
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Name of the trigger
    #[clap(long, alias = "name")]
    title: Option<String>,

    /// ID of the template (already uploaded to Fiberplane)
    #[clap(long)]
    template_id: Option<Base64Uuid>,

    /// Default arguments to be passed to the template when the trigger is invoked
    /// Can be passed as a JSON object or as a comma-separated list of key=value pairs
    #[clap(long)]
    default_arguments: Option<TemplateArguments>,

    /// Output of the trigger
    #[clap(long, short, default_value = "table", value_enum)]
    output: TriggerOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct GetArguments {
    /// Trigger ID
    trigger_id: Option<Base64Uuid>,

    /// Output of the trigger
    #[clap(long, short, default_value = "table", value_enum)]
    output: TriggerOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct DeleteArguments {
    /// Trigger ID
    trigger_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct ListArguments {
    /// Workspace to list the triggers for
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the triggers
    #[clap(long, short, default_value = "table", value_enum)]
    output: TriggerOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct InvokeArguments {
    /// Trigger ID
    #[clap(long, short)]
    trigger_id: Option<Base64Uuid>,

    /// Secret Key (returned when the trigger is initially created)
    #[clap(long, short)]
    secret_key: Option<String>,

    /// Output of the triggers
    #[clap(long, short, default_value = "table", value_enum)]
    output: TriggerOutput,

    /// Values to inject into the template.
    ///
    /// Can be passed as a JSON object or as a comma-separated list of key=value pairs
    template_arguments: Option<TemplateArguments>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

/// A generic output for trigger related commands.
#[derive(ValueEnum, Clone)]
enum TriggerOutput {
    /// Output the result as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

async fn handle_trigger_create_command(args: CreateArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let default_arguments = if let Some(default_arguments) = args.default_arguments {
        Some(serde_json::to_value(default_arguments)?)
    } else {
        None
    };

    let workspace_id = interactive::workspace_picker(&config, args.workspace_id).await?;
    let template_id =
        interactive::template_picker(&config, args.template_id, Some(workspace_id)).await?;
    let title = interactive::text_req("Title", args.title, None)?;

    let trigger = NewTrigger {
        title,
        default_arguments,
        template_id: template_id.to_string(),
    };
    let trigger = trigger_create(&config, &workspace_id.to_string(), trigger)
        .await
        .with_context(|| "Error creating trigger")?;

    match args.output {
        TriggerOutput::Table => {
            let trigger = GenericKeyValue::from_trigger(trigger, args.base_url);
            output_details(trigger)
        }
        TriggerOutput::Json => output_json(&trigger),
    }
}

async fn handle_trigger_get_command(args: GetArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let trigger_id = interactive::trigger_picker(&config, args.trigger_id, None).await?;

    let trigger = trigger_get(&config, &trigger_id.to_string())
        .await
        .with_context(|| "Error getting trigger details")?;

    match args.output {
        TriggerOutput::Table => {
            output_details(GenericKeyValue::from_trigger(trigger, args.base_url))
        }
        TriggerOutput::Json => output_json(&trigger),
    }
}

async fn handle_trigger_delete_command(args: DeleteArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let trigger_id = interactive::trigger_picker(&config, args.trigger_id, None).await?;

    trigger_delete(&config, &trigger_id.to_string())
        .await
        .with_context(|| "Error deleting trigger")?;

    info!("Deleted trigger");

    Ok(())
}

async fn handle_trigger_list_command(args: ListArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = interactive::workspace_picker(&config, args.workspace_id).await?;
    let mut triggers = trigger_list(&config, &workspace_id.to_string())
        .await
        .with_context(|| "Error getting triggers")?;

    triggers.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    match args.output {
        TriggerOutput::Table => {
            let triggers: Vec<TriggerRow> = triggers.into_iter().map(Into::into).collect();

            output_list(triggers)
        }
        TriggerOutput::Json => output_json(&triggers),
    }
}

async fn handle_trigger_invoke_command(args: InvokeArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let trigger_id = interactive::trigger_picker(&config, args.trigger_id, None).await?;
    let secret_key = interactive::text_req("Secret Key", args.secret_key, None)?;

    let body = serde_json::to_value(&args.template_arguments)?;

    let config = Configuration {
        base_path: args.base_url.to_string(),
        ..Configuration::default()
    };
    let response = trigger_invoke(&config, &trigger_id.to_string(), &secret_key, Some(body))
        .await
        .with_context(|| "Error invoking trigger")?;

    match args.output {
        TriggerOutput::Table => {
            let response = GenericKeyValue::from_trigger_invoke_response(response);
            output_details(response)
        }
        TriggerOutput::Json => output_json(&response),
    }
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

impl GenericKeyValue {
    pub fn from_trigger(trigger: Trigger, base_url: Url) -> Vec<GenericKeyValue> {
        let invoke_url = format!(
            "{}api/triggers/{}/{}",
            base_url,
            trigger.id,
            trigger
                .secret_key
                .unwrap_or_else(|| String::from("<secret_key>"))
        );

        vec![
            GenericKeyValue::new("Title:", trigger.title),
            GenericKeyValue::new("ID:", trigger.id),
            GenericKeyValue::new("Invoke URL:", invoke_url),
            GenericKeyValue::new("Template ID:", trigger.template_id),
        ]
    }

    pub fn from_trigger_invoke_response(response: TriggerInvokeResponse) -> Vec<GenericKeyValue> {
        vec![
            GenericKeyValue::new("Notebook Title:", response.notebook_title),
            GenericKeyValue::new("Notebook URL:", response.notebook_url),
            GenericKeyValue::new("Notebook ID:", response.notebook_id),
        ]
    }
}
