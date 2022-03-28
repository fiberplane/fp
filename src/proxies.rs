use crate::config::api_client_configuration;
use crate::{output_details, output_list, GenericKeyValue};
use anyhow::{anyhow, Result};
use clap::Parser;
use cli_table::{Table, WithTitle};
use fp_api_client::apis::default_api::{
    proxy_create, proxy_data_sources_list, proxy_delete, proxy_get, proxy_list,
};
use fp_api_client::models::{DataSourceAndProxySummary, NewProxy, ProxySummary};
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

    output_details(proxy.table())
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

    output_list(proxies.with_title())
}

async fn handle_get_command(args: SingleProxyArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let proxy = proxy_get(&config, &args.proxy_id).await?;

    let proxy = GenericKeyValue::from_proxy(proxy);

    output_details(proxy.table())
}

async fn handle_data_sources_command(args: GlobalArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let data_sources = proxy_data_sources_list(&config).await?;

    let data_sources: Vec<DataSourceAndProxySummaryRow> =
        data_sources.into_iter().map(Into::into).collect();

    output_list(data_sources.with_title())
}

async fn handle_remove_command(args: SingleProxyArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    proxy_delete(&config, &args.proxy_id).await?;
    info!("Removed proxy");
    Ok(())
}

#[derive(Table)]
struct ProxySummaryRow {
    #[table(title = "ID")]
    id: String,

    #[table(title = "Name")]
    name: String,

    #[table(title = "Status")]
    status: String,
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
struct DataSourceAndProxySummaryRow {
    #[table(title = "Name")]
    name: String,

    #[table(title = "Type")]
    _type: String,

    #[table(title = "Proxy name")]
    proxy_name: String,

    #[table(title = "Proxy ID")]
    proxy_id: String,

    #[table(title = "Proxy status")]
    proxy_status: String,
}

impl From<DataSourceAndProxySummary> for DataSourceAndProxySummaryRow {
    fn from(data_source_and_proxy_summary: DataSourceAndProxySummary) -> Self {
        Self {
            name: data_source_and_proxy_summary.name,
            _type: data_source_and_proxy_summary._type.to_string(),
            proxy_name: data_source_and_proxy_summary.proxy.name,
            proxy_id: data_source_and_proxy_summary.proxy.id,
            proxy_status: data_source_and_proxy_summary.proxy.status.to_string(),
        }
    }
}
