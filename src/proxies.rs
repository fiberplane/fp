use crate::config::api_client_configuration;
use crate::interactive;
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use anyhow::{anyhow, Result};
use base64uuid::Base64Uuid;
use clap::{ArgEnum, Parser};
use cli_table::Table;
use fp_api_client::apis::default_api::{
    proxy_create, proxy_data_sources_list, proxy_delete, proxy_get, proxy_list,
};
use fp_api_client::models::{DataSourceAndProxySummary, NewProxy, Proxy, ProxySummary};
use petname::petname;
use std::cmp::Ordering;
use std::path::PathBuf;
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
    Create(CreateArgs),

    /// List all proxies
    List(ListArgs),

    /// List all data sources
    #[clap(alias = "datasources")]
    DataSources(DataSourcesArgs),

    /// Retrieve a single proxy
    Get(GetArgs),

    /// Delete a proxy
    Delete(DeleteArgs),
}

#[derive(Parser)]
pub struct CreateArgs {
    /// Proxy name, leave empty to auto-generate a name
    name: Option<String>,

    /// Output of the proxy
    #[clap(long, short, default_value = "table", arg_enum)]
    output: ProxyOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct ListArgs {
    /// Output of the proxy
    #[clap(long, short, default_value = "table", arg_enum)]
    output: ProxyOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct DataSourcesArgs {
    /// Output of the proxy
    #[clap(long, short, default_value = "table", arg_enum)]
    output: ProxyOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct GetArgs {
    /// ID of the proxy
    proxy_id: Option<Base64Uuid>,

    /// Output of the proxy
    #[clap(long, short, default_value = "table", arg_enum)]
    output: ProxyOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct DeleteArgs {
    /// ID of the proxy
    proxy_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

/// A generic output for proxy related commands.
#[derive(ArgEnum, Clone)]
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
    let name = args.name.unwrap_or_else(|| petname(2, "-"));
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let proxy = proxy_create(&config, NewProxy { name })
        .await
        .map_err(|e| anyhow!(format!("Error adding proxy: {:?}", e)))?;

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

async fn handle_list_command(args: ListArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let mut proxies = proxy_list(&config).await?;

    match args.output {
        ProxyOutput::Table => {
            // Show connected proxies first, and then sort alphabetically by name
            proxies.sort_by(|a, b| {
                use fp_api_client::models::ProxyConnectionStatus::*;
                match (a.status, b.status) {
                    (Connected, Disconnected) => Ordering::Less,
                    (Disconnected, Connected) => Ordering::Greater,
                    _ => a.name.cmp(&b.name),
                }
            });

            let proxies: Vec<ProxySummaryRow> = proxies.into_iter().map(Into::into).collect();

            output_list(proxies)
        }
        ProxyOutput::Json => output_json(&proxies),
    }
}

async fn handle_get_command(args: GetArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let proxy_id = interactive::proxy_picker(&config, args.proxy_id).await?;

    let proxy = proxy_get(&config, &proxy_id.to_string()).await?;

    match args.output {
        ProxyOutput::Table => {
            let proxy = GenericKeyValue::from_proxy(proxy);
            output_details(proxy)
        }
        ProxyOutput::Json => output_json(&proxy),
    }
}

async fn handle_data_sources_command(args: DataSourcesArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let data_sources = proxy_data_sources_list(&config).await?;

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
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let proxy_id = interactive::proxy_picker(&config, args.proxy_id).await?;

    proxy_delete(&config, &proxy_id.to_string()).await?;

    info!("Removed proxy");
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
}

impl From<ProxySummary> for ProxySummaryRow {
    fn from(proxy: ProxySummary) -> Self {
        Self {
            id: proxy.id,
            name: proxy.name,
            status: proxy.status.to_string(),
        }
    }
}

#[derive(Table)]
pub struct DataSourceAndProxySummaryRow {
    #[table(title = "Name")]
    pub name: String,

    #[table(title = "Type")]
    pub _type: String,

    #[table(title = "Status")]
    pub status: String,

    #[table(title = "Proxy name")]
    pub proxy_name: String,

    #[table(title = "Proxy ID")]
    pub proxy_id: String,

    #[table(title = "Proxy status")]
    pub proxy_status: String,
}

impl From<DataSourceAndProxySummary> for DataSourceAndProxySummaryRow {
    fn from(data_source_and_proxy_summary: DataSourceAndProxySummary) -> Self {
        Self {
            name: data_source_and_proxy_summary.name,
            _type: data_source_and_proxy_summary._type.to_string(),
            status: data_source_and_proxy_summary
                .error_message
                .unwrap_or_else(|| "connected".to_string()),
            proxy_name: data_source_and_proxy_summary.proxy.name,
            proxy_id: data_source_and_proxy_summary.proxy.id,
            proxy_status: data_source_and_proxy_summary.proxy.status.to_string(),
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
                .map(|datasource| format!("{} ({:?})", datasource.name, datasource._type))
                .collect::<Vec<String>>()
                .join("\n")
        };

        vec![
            GenericKeyValue::new("Name:", proxy.name),
            GenericKeyValue::new("ID:", proxy.id),
            GenericKeyValue::new("Status:", proxy.status.to_string()),
            GenericKeyValue::new("Datasources:", data_sources),
        ]
    }
}
