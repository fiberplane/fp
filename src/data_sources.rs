use anyhow::{anyhow, Result};
use clap::{Parser, ValueEnum};
use cli_table::Table;
use fiberplane::api_client::apis::default_api::{
    data_source_create, data_source_delete, data_source_list, data_source_update,
};
use fiberplane::api_client::models::{DataSource, NewDataSource, UpdateDataSource};
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::names::Name;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{path::PathBuf, str::FromStr};
use url::Url;

use crate::config::api_client_configuration;
use crate::interactive::{data_source_picker, name_req, text_opt, text_req, workspace_picker};
use crate::output::{output_details, output_list, GenericKeyValue};
use crate::workspaces;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    /// Create a new workspace data source
    Create(CreateArgs),

    /// View and modify the default data sources for the workspace
    #[clap(subcommand, alias = "default")]
    Defaults(workspaces::DefaultDataSourcesSubCommand),

    /// Delete a workspace data source
    Delete(DeleteArgs),

    /// Get the details of a workspace data source
    Get(GetArgs),

    /// List all workspace data sources
    List(ListArgs),

    /// Update a data source
    Update(UpdateArgs),
}

#[derive(ValueEnum, Clone, Debug)]
enum DataSourceOutput {
    /// Output the values as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
struct ProviderConfig(Map<String, Value>);

impl FromStr for ProviderConfig {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let map = serde_json::from_str(s)?;
        Ok(ProviderConfig(map))
    }
}

#[derive(Parser)]
struct CreateArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Name of the data source
    #[clap(short, long)]
    name: Option<Name>,

    /// Description of the data source
    #[clap(short, long)]
    description: Option<String>,

    /// Provider type of the data source
    #[clap(short, long)]
    provider_type: Option<String>,

    /// Provider configuration
    #[clap(long)]
    provider_config: Option<ProviderConfig>,

    /// Output of the notebook
    #[clap(long, short, default_value = "table", value_enum)]
    output: DataSourceOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct GetArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Name of the data source
    #[clap(short, long)]
    name: Option<Name>,

    /// Output of the notebook
    #[clap(long, short, default_value = "table", value_enum)]
    output: DataSourceOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct DeleteArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Name of the data source
    #[clap(short, long)]
    name: Option<Name>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct UpdateArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Name of the data source to update
    #[clap(short, long)]
    name: Option<Name>,

    /// New description of the data source
    #[clap(short, long)]
    description: Option<String>,

    /// New provider configuration
    #[clap(long)]
    provider_config: Option<ProviderConfig>,

    /// Output format
    #[clap(long, short, default_value = "table", value_enum)]
    output: DataSourceOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct ListArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the notebook
    #[clap(long, short, default_value = "table", value_enum)]
    output: DataSourceOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Create(args) => handle_create(args).await,
        SubCommand::Defaults(sub_command) => {
            workspaces::handle_default_data_sources_command(sub_command).await
        }
        SubCommand::Delete(args) => handle_delete(args).await,
        SubCommand::Get(args) => handle_get(args).await,
        SubCommand::List(args) => handle_list(args).await,
        SubCommand::Update(args) => handle_update(args).await,
    }
}

async fn handle_create(args: CreateArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id)
        .await?
        .to_string();
    let name = name_req("Data source name", args.name, None)?;
    let description = text_opt("Description", args.description, None);
    let provider_type = text_req(
        "Provider type (prometheus, elasticsearch, etc)",
        args.provider_type,
        None,
    )?;
    let provider_config = text_req(
        r#"Provider config in JSON (e.g.e {"url": "..."})"#,
        args.provider_config
            .and_then(|c| serde_json::to_string(&c.0).ok()),
        None,
    )?;
    let provider_config = ProviderConfig::from_str(&provider_config)
        .map_err(|e| anyhow!("Error parsing provider config as JSON: {:?}", e))?;

    // We are creating a direct (non-proxied) data-source, so we can hard-code
    // the version to be `2`. Studio does contain some legacy providers still
    // at the time of writing, but will emulate the new protocol version anyway.
    let protocol_version = 2;

    let data_source = NewDataSource {
        name: name.to_string(),
        description,
        protocol_version,
        provider_type,
        config: Value::Object(provider_config.0),
    };
    let data_source = data_source_create(&config, &workspace_id, data_source).await?;

    match args.output {
        DataSourceOutput::Table => output_details(GenericKeyValue::from_data_source(&data_source)),
        DataSourceOutput::Json => {
            println!("{}", serde_json::to_string_pretty(&data_source)?);
            Ok(())
        }
    }
}

async fn handle_delete(args: DeleteArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    let data_source =
        data_source_picker(&config, Some(workspace_id), args.name.map(String::from)).await?;

    data_source_delete(&config, &workspace_id.to_string(), &data_source.name).await?;

    Ok(())
}

async fn handle_get(args: GetArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    let data_source =
        data_source_picker(&config, Some(workspace_id), args.name.map(String::from)).await?;

    match args.output {
        DataSourceOutput::Table => output_details(GenericKeyValue::from_data_source(&data_source)),
        DataSourceOutput::Json => {
            println!("{}", serde_json::to_string_pretty(&data_source)?);
            Ok(())
        }
    }
}

async fn handle_update(args: UpdateArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    let data_source =
        data_source_picker(&config, Some(workspace_id), args.name.map(String::from)).await?;

    let update = UpdateDataSource {
        description: args.description,
        config: args.provider_config.map(|c| Value::Object(c.0)),
    };

    let data_source = data_source_update(
        &config,
        &workspace_id.to_string(),
        &data_source.name,
        update,
    )
    .await?;

    match args.output {
        DataSourceOutput::Table => output_details(GenericKeyValue::from_data_source(&data_source)),
        DataSourceOutput::Json => {
            println!("{}", serde_json::to_string_pretty(&data_source)?);
            Ok(())
        }
    }
}

async fn handle_list(args: ListArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    let data_sources = data_source_list(&config, &workspace_id.to_string()).await?;

    match args.output {
        DataSourceOutput::Table => {
            let data_sources = data_sources.into_iter().map(DataSourceRow::from).collect();
            output_list(data_sources)
        }
        DataSourceOutput::Json => {
            println!("{}", serde_json::to_string_pretty(&data_sources)?);
            Ok(())
        }
    }
}

impl GenericKeyValue {
    pub fn from_data_source(data_source: &DataSource) -> Vec<GenericKeyValue> {
        vec![
            GenericKeyValue::new("Name", &data_source.name),
            GenericKeyValue::new(
                "Description",
                data_source.description.clone().unwrap_or_default(),
            ),
            GenericKeyValue::new("Provider Type", &data_source.provider_type),
            GenericKeyValue::new(
                "Config",
                &data_source
                    .config
                    .as_ref()
                    .and_then(|c| serde_json::to_string(&c).ok())
                    .unwrap_or_default(),
            ),
            GenericKeyValue::new(
                "Created At",
                data_source.created_at.clone().unwrap_or_default(),
            ),
            GenericKeyValue::new(
                "Updated At",
                data_source.updated_at.clone().unwrap_or_default(),
            ),
        ]
    }
}

#[derive(Table)]
pub struct DataSourceRow {
    #[table(title = "Name")]
    pub name: String,

    #[table(title = "Proxy Name")]
    pub proxy_name: String,

    #[table(title = "Provider Type")]
    pub provider_type: String,

    #[table(title = "Updated at")]
    pub updated_at: String,

    #[table(title = "Created at")]
    pub created_at: String,
}

impl From<DataSource> for DataSourceRow {
    fn from(data_source: DataSource) -> Self {
        Self {
            name: data_source.name,
            proxy_name: data_source.proxy_name.unwrap_or_default(),
            provider_type: data_source.provider_type,
            updated_at: data_source.updated_at.unwrap_or_default(),
            created_at: data_source.created_at.unwrap_or_default(),
        }
    }
}
