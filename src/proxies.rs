use crate::config::api_client_configuration;
use crate::output::{output_details, output_list, GenericKeyValue};
use anyhow::{anyhow, Result};
use clap::Parser;
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
    #[clap()]
    Create(CreateArgs),

    /// List all proxies
    #[clap()]
    List(GlobalArgs),

    /// List all data sources
    #[clap(alias = "datasources")]
    DataSources(GlobalArgs),

    /// Retrieve a single proxy
    #[clap()]
    Get(SingleProxyArgs),

    /// Remove a proxy
    #[clap()]
    Remove(SingleProxyArgs),
}

#[derive(Parser)]
pub struct CreateArgs {
    /// Proxy name, leave empty to auto-generate a name
    #[clap()]
    name: Option<String>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct GlobalArgs {
    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct SingleProxyArgs {
    /// ID of the proxy
    #[clap()]
    proxy_id: String,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Create(args) => handle_add_command(args).await,
        List(args) => handle_list_command(args).await,
        Get(args) => handle_get_command(args).await,
        DataSources(args) => handle_data_sources_command(args).await,
        Remove(args) => handle_remove_command(args).await,
    }
}

async fn handle_add_command(args: CreateArgs) -> Result<()> {
    let name = args.name.unwrap_or_else(|| petname(2, "-"));
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let proxy = proxy_create(&config, Some(NewProxy { name }))
        .await
        .map_err(|e| anyhow!(format!("Error adding proxy: {:?}", e)))?;

    let token = proxy
        .token
        .clone()
        .ok_or_else(|| anyhow!("Create proxy endpoint should have returned an API token"))?;

    let mut proxy = GenericKeyValue::from_proxy(proxy);
    proxy.push(GenericKeyValue::new("Token", token));

    output_details(proxy)
}

async fn handle_list_command(args: GlobalArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let mut proxies = proxy_list(&config).await?;

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

async fn handle_get_command(args: SingleProxyArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let proxy = proxy_get(&config, &args.proxy_id).await?;

    let proxy = GenericKeyValue::from_proxy(proxy);

    output_details(proxy)
}

async fn handle_data_sources_command(args: GlobalArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let data_sources = proxy_data_sources_list(&config).await?;

    let data_sources: Vec<DataSourceAndProxySummaryRow> =
        data_sources.into_iter().map(Into::into).collect();

    output_list(data_sources)
}

async fn handle_remove_command(args: SingleProxyArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    proxy_delete(&config, &args.proxy_id).await?;
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
        let datasources = if proxy.data_sources.is_empty() {
            String::from("(none)")
        } else {
            let mut datasources = String::new();
            for datasource in proxy.data_sources {
                datasources.push_str(&format!("{} ({:?})\n", datasource.name, datasource._type))
            }
            datasources
        };

        vec![
            GenericKeyValue::new("Name:", proxy.name),
            GenericKeyValue::new("ID:", proxy.id),
            GenericKeyValue::new("Status:", proxy.status.to_string()),
            GenericKeyValue::new("Datasources:", datasources),
        ]
    }
}
