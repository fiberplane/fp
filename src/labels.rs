use crate::config::api_client_configuration;
use crate::interactive;
use crate::output::{output_json, output_string_list};
use anyhow::Result;
use clap::{ArgEnum, Parser};
use fp_api_client::apis::default_api::{label_keys_list, label_values_list};
use std::path::PathBuf;
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
    #[clap(long, short)]
    prefix: Option<String>,

    /// Output of the notebook
    #[clap(long, short, default_value = "list", arg_enum)]
    output: ListKeysOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(ArgEnum, Clone)]
enum ListKeysOutput {
    /// Output the keys as a list
    List,

    /// Output the result as a JSON encoded object
    Json,
}

async fn handle_list_keys_command(args: ListKeysArgs) -> Result<()> {
    use ListKeysOutput::*;

    let prefix = interactive::text_opt("Prefix", args.prefix, None);

    let config = api_client_configuration(args.config, &args.base_url).await?;
    let keys = label_keys_list(&config, prefix.as_deref()).await?;

    match args.output {
        List => output_string_list(keys),
        Json => output_json(&keys),
    }
}

#[derive(Parser)]
pub struct ListValuesArgs {
    label_key: Option<String>,

    #[clap(long, short)]
    prefix: Option<String>,

    /// Output of the notebook
    #[clap(long, short, default_value = "list", arg_enum)]
    output: ListValuesOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(ArgEnum, Clone)]
enum ListValuesOutput {
    /// Output the values as a list
    List,

    /// Output the result as a JSON encoded object
    Json,
}

async fn handle_list_values_command(args: ListValuesArgs) -> Result<()> {
    use ListValuesOutput::*;

    let config = api_client_configuration(args.config, &args.base_url).await?;

    let label_key = interactive::text_req("Label key", args.label_key, None)?;
    let prefix = interactive::text_opt("Prefix", args.prefix, None);

    let values = label_values_list(&config, &label_key, prefix.as_deref()).await?;

    match args.output {
        List => output_string_list(values),
        Json => output_json(&values),
    }
}
