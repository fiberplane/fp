use crate::config::api_client_configuration;
use crate::interactive::{self, name_req, workspace_picker};
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use anyhow::{anyhow, Result};
use clap::{Parser, ValueEnum};
use cli_table::Table;
use fiberplane::api_client::{data_source_list, proxy_create, proxy_delete, proxy_get, proxy_list};
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::data_sources::{DataSource, DataSourceStatus};
use fiberplane::models::names::Name;
use fiberplane::models::proxies::{NewProxy, Proxy, ProxySummary};
use petname::petname;
use serde::Serialize;
use std::{cmp::Ordering, collections::BTreeMap, path::PathBuf};
use tracing::info;
use url::Url;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
pub enum SubCommand {
    /// Create a new Proxy
    #[clap(alias = "add")]
    Create(CreateArgs),

    /// List all proxies
    List(ListArgs),

    /// List all data sources
    #[clap(alias = "datasources")]
    DataSources(DataSourcesArgs),

    /// Retrieve a single proxy
    Get(GetArgs),

    /// Delete a proxy
    #[clap(aliases = &["remove", "rm"])]
    Delete(DeleteArgs),
}

#[derive(Parser)]
pub struct CreateArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Proxy name, leave empty to auto-generate a name
    name: Option<Name>,

    description: Option<String>,

    /// Output of the proxy
    #[clap(long, short, default_value = "table", value_enum)]
    output: ProxyOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct ListArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the proxy
    #[clap(long, short, default_value = "table", value_enum)]
    output: ProxyOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct DataSourcesArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the proxy
    #[clap(long, short, default_value = "table", value_enum)]
    output: ProxyOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct GetArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// ID of the proxy
    proxy_name: Option<Name>,

    /// Output of the proxy
    #[clap(long, short, default_value = "table", value_enum)]
    output: ProxyOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct DeleteArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Name of the proxy
    proxy_name: Option<Name>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

/// A generic output for proxy related commands.
#[derive(ValueEnum, Clone)]
enum ProxyOutput {
    /// Output the result as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Create(args) => handle_create_command(args).await,
        List(args) => handle_list_command(args).await,
        Get(args) => handle_get_command(args).await,
        DataSources(args) => handle_data_sources_command(args).await,
        Delete(args) => handle_delete_command(args).await,
    }
}

async fn handle_create_command(args: CreateArgs) -> Result<()> {
    let default_name = Name::new(petname(2, "-")).expect("petname should be valid name");
    let name = name_req("Proxy name", args.name, Some(default_name))?;
    let client = api_client_configuration(args.config, args.base_url).await?;
    let workspace_id = workspace_picker(&client, args.workspace_id).await?;

    let proxy = proxy_create(
        &client,
        workspace_id,
        NewProxy::builder()
            .name(name)
            .description(args.description)
            .build(),
    )
    .await
    .map_err(|e| anyhow!("Error adding proxy: {:?}", e))?;

    match args.output {
        ProxyOutput::Table => {
            let token = proxy.token.clone().ok_or_else(|| {
                anyhow!("Create proxy endpoint should have returned an API token")
            })?;
            let mut proxy = GenericKeyValue::from_proxy(proxy);
            proxy.push(GenericKeyValue::new("Token", token));
            output_details(proxy)
        }
        ProxyOutput::Json => output_json(&proxy),
    }
}

#[derive(Debug, Clone, Serialize)]
struct ProxySummaryWithConnectedDataSources {
    #[serde(flatten)]
    proxy: ProxySummary,
    connected_data_sources: usize,
    total_data_sources: usize,
}

async fn handle_list_command(args: ListArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;
    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let proxies = proxy_list(&client, workspace_id).await?;
    let data_sources = data_source_list(&client, workspace_id).await?;

    // Put all of the proxies in a map so we can easily look them up by ID and add the data source counts
    let mut proxies: BTreeMap<String, ProxySummaryWithConnectedDataSources> = proxies
        .into_iter()
        .map(|proxy| {
            (
                proxy.name.to_string(),
                ProxySummaryWithConnectedDataSources {
                    proxy,
                    connected_data_sources: 0,
                    total_data_sources: 0,
                },
            )
        })
        .collect();
    // Count the total and connected data sources for each proxy
    for data_source in data_sources {
        if let Some(proxy_name) = data_source.proxy_name {
            if let Some(proxy) = proxies.get_mut(proxy_name.as_str()) {
                proxy.total_data_sources += 1;
                if data_source.status == Some(DataSourceStatus::Connected) {
                    proxy.connected_data_sources += 1;
                }
            }
        }
    }
    let mut proxies: Vec<ProxySummaryWithConnectedDataSources> = proxies.into_values().collect();

    match args.output {
        ProxyOutput::Table => {
            // Show connected proxies first, and then sort by the number of data sources
            proxies.sort_by(|a, b| {
                use fiberplane::models::proxies::ProxyStatus::*;
                match (a.proxy.status, b.proxy.status) {
                    (Connected, Disconnected) => Ordering::Less,
                    (Disconnected, Connected) => Ordering::Greater,
                    (Connected, Connected) => {
                        b.connected_data_sources.cmp(&a.connected_data_sources)
                    }
                    (Disconnected, Disconnected) => b.total_data_sources.cmp(&a.total_data_sources),
                    (_, _) => panic!(
                        "Unknown proxy status: {:?}, {:?}",
                        a.proxy.status, b.proxy.status
                    ),
                }
            });

            let proxies: Vec<ProxySummaryRow> = proxies.into_iter().map(Into::into).collect();

            output_list(proxies)
        }
        ProxyOutput::Json => output_json(&proxies),
    }
}

async fn handle_get_command(args: GetArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;
    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let proxy_name =
        interactive::proxy_picker(&client, Some(workspace_id), args.proxy_name.map(Into::into))
            .await?;

    let proxy = proxy_get(&client, workspace_id, &proxy_name).await?;

    match args.output {
        ProxyOutput::Table => {
            let proxy = GenericKeyValue::from_proxy(proxy);
            output_details(proxy)
        }
        ProxyOutput::Json => output_json(&proxy),
    }
}

async fn handle_data_sources_command(args: DataSourcesArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;
    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let data_sources = data_source_list(&client, workspace_id).await?;

    match args.output {
        ProxyOutput::Table => {
            let data_sources: Vec<DataSourceAndProxySummaryRow> =
                data_sources.into_iter().map(Into::into).collect();

            output_list(data_sources)
        }
        ProxyOutput::Json => output_json(&data_sources),
    }
}

async fn handle_delete_command(args: DeleteArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;
    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let proxy_name =
        interactive::proxy_picker(&client, Some(workspace_id), args.proxy_name.map(Into::into))
            .await?;

    proxy_delete(&client, workspace_id, &proxy_name).await?;

    info!("Deleted proxy");
    Ok(())
}

#[derive(Table)]
pub struct ProxySummaryRow {
    #[table(title = "Name")]
    pub name: String,

    #[table(title = "ID")]
    pub id: String,

    #[table(title = "Status")]
    pub status: String,

    #[table(title = "Connected Data Sources")]
    pub data_sources_connected: String,
}

impl From<ProxySummaryWithConnectedDataSources> for ProxySummaryRow {
    fn from(proxy: ProxySummaryWithConnectedDataSources) -> Self {
        Self {
            name: proxy.proxy.name.to_string(),
            id: proxy.proxy.id.to_string(),
            status: proxy.proxy.status.to_string(),
            data_sources_connected: format!(
                "{} / {}",
                proxy.connected_data_sources, proxy.total_data_sources
            ),
        }
    }
}

#[derive(Table)]
pub struct DataSourceAndProxySummaryRow {
    #[table(title = "Name")]
    pub name: String,

    #[table(title = "Proxy Name")]
    pub proxy_name: String,

    #[table(title = "Provider Type")]
    pub provider_type: String,

    #[table(title = "Status")]
    pub status: String,
}

impl From<DataSource> for DataSourceAndProxySummaryRow {
    fn from(data_source: DataSource) -> Self {
        let status = match data_source.status {
            Some(DataSourceStatus::Connected) => "Connected".to_string(),
            Some(DataSourceStatus::Error { .. }) => "Error".to_string(),
            None => String::new(),
            Some(_) => panic!("Unknown DataSourceStatus: {:?}", data_source.status),
        };

        Self {
            name: data_source.name.to_string(),
            provider_type: data_source.provider_type,
            status,
            proxy_name: data_source
                .proxy_name
                .map_or_else(String::new, |name| name.to_string()),
        }
    }
}

impl GenericKeyValue {
    pub fn from_proxy(proxy: Proxy) -> Vec<GenericKeyValue> {
        let data_sources = if proxy.data_sources.is_empty() {
            String::from("(none)")
        } else {
            proxy
                .data_sources
                .iter()
                .map(|datasource| {
                    format!(
                        "{} ({}): {}{}",
                        datasource.name,
                        datasource.provider_type,
                        datasource
                            .status
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_default(),
                        if let Some(DataSourceStatus::Error(error)) = &datasource.status {
                            format!(" - {}", serde_json::to_string(error).unwrap())
                        } else {
                            String::new()
                        }
                    )
                })
                .collect::<Vec<String>>()
                .join("\n")
        };

        vec![
            GenericKeyValue::new("Name:", proxy.name),
            GenericKeyValue::new("ID:", proxy.id),
            GenericKeyValue::new("Status:", proxy.status.to_string()),
            GenericKeyValue::new("Data sources:", data_sources),
        ]
    }
}
