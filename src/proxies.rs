use crate::config::api_client_configuration;
use crate::{
    default_detail_border, default_detail_separator, default_list_separator, GenericKeyValue,
};
use anyhow::{anyhow, Result};
use clap::Parser;
use cli_table::{print_stdout, Table, WithTitle};
use fp_api_client::apis::default_api::{
    proxy_create, proxy_data_sources_list, proxy_delete, proxy_get, proxy_list,
};
use fp_api_client::models::{NewProxy, ProxySummary};
use petname::petname;
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
    #[clap()]
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
        .ok_or_else(|| anyhow!("Create proxy endpoint should have returned an API token"))?;

    info!("Added proxy \"{}\"", proxy.name);
    info!("Proxy API Token: {}", token);
    Ok(())
}

#[derive(Table)]
struct ProxyList {
    #[table(title = "ID")]
    id: String,

    #[table(title = "Name")]
    name: String,

    #[table(title = "Status")]
    status: String,
}

impl From<ProxySummary> for ProxyList {
    fn from(proxy: ProxySummary) -> Self {
        Self {
            id: proxy.id,
            name: proxy.name,
            status: proxy.status.to_string(),
        }
    }
}

async fn handle_list_command(args: GlobalArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let mut proxies: Vec<ProxyList> = proxy_list(&config)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();

    // Show connected proxies first, and then sort alphabetically by name
    proxies.sort_by(|a, b| a.name.cmp(&b.name));

    print_stdout(proxies.with_title().separator(default_list_separator())).map_err(Into::into)
}

async fn handle_get_command(args: SingleProxyArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let proxy = proxy_get(&config, &args.proxy_id).await?;

    let proxy = GenericKeyValue::from_proxy(proxy);

    print_stdout(
        proxy
            .table()
            .border(default_detail_border())
            .separator(default_detail_separator()),
    )
    .map_err(Into::into)
}

async fn handle_data_sources_command(args: GlobalArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let data_sources = proxy_data_sources_list(&config).await?;

    // TODO should we print something if there are no data sources?
    for data_source in data_sources {
        println!(
            "- {} (Type: {:?}, Proxy: {}, Proxy ID: {}, Proxy Status: {:?})",
            data_source.name,
            data_source._type,
            data_source.proxy.name,
            data_source.proxy.id,
            data_source.proxy.status
        );
    }

    Ok(())
}

async fn handle_remove_command(args: SingleProxyArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    proxy_delete(&config, &args.proxy_id).await?;
    info!("Removed proxy");
    Ok(())
}
