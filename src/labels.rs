use crate::config::api_client_configuration;
use crate::interactive::{self, workspace_picker};
use crate::output::{output_json, output_string_list};
use anyhow::Result;
use clap::{Parser, ValueEnum};
use fiberplane::base64uuid::Base64Uuid;
use url::Url;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
pub enum SubCommand {
    /// List all unique labels keys that are used.
    ListKeys(ListKeysArgs),

    /// List all unique labels values that are used for a specific label key.
    ListValues(ListValuesArgs),
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        ListKeys(args) => handle_list_keys_command(args).await,
        ListValues(args) => handle_list_values_command(args).await,
    }
}

#[derive(Parser)]
pub struct ListKeysArgs {
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(long, short)]
    prefix: Option<String>,

    /// Output of the notebook
    #[clap(long, short, default_value = "list", value_enum)]
    output: ListKeysOutput,

    /// Workspace to use

    #[clap(from_global)]
    base_url: Option<Url>,

    #[clap(from_global)]
    profile: Option<String>,

    #[clap(from_global)]
    token: Option<String>,
}

#[derive(ValueEnum, Clone)]
enum ListKeysOutput {
    /// Output the keys as a list
    List,

    /// Output the result as a JSON encoded object
    Json,
}

async fn handle_list_keys_command(args: ListKeysArgs) -> Result<()> {
    let client = api_client_configuration(args.token, args.profile, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let prefix = interactive::text_opt("Prefix", args.prefix, None);

    let keys = client
        .label_keys_list(workspace_id, prefix.as_deref())
        .await?;

    match args.output {
        ListKeysOutput::List => output_string_list(keys),
        ListKeysOutput::Json => output_json(&keys),
    }
}

#[derive(Parser)]
pub struct ListValuesArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    label_key: Option<String>,

    #[clap(long, short)]
    prefix: Option<String>,

    /// Output of the notebook
    #[clap(long, short, default_value = "list", value_enum)]
    output: ListValuesOutput,

    #[clap(from_global)]
    base_url: Option<Url>,

    #[clap(from_global)]
    profile: Option<String>,

    #[clap(from_global)]
    token: Option<String>,
}

#[derive(ValueEnum, Clone)]
enum ListValuesOutput {
    /// Output the values as a list
    List,

    /// Output the result as a JSON encoded object
    Json,
}

async fn handle_list_values_command(args: ListValuesArgs) -> Result<()> {
    let client = api_client_configuration(args.token, args.profile, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let label_key = interactive::text_req("Label key", args.label_key, None)?;
    let prefix = interactive::text_opt("Prefix", args.prefix, None);

    let values = client
        .label_values_list(workspace_id, &label_key, prefix.as_deref())
        .await?;

    match args.output {
        ListValuesOutput::List => output_string_list(values),
        ListValuesOutput::Json => output_json(&values),
    }
}
