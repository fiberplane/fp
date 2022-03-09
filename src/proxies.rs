use crate::config::api_client_configuration;
use anyhow::{anyhow, Result};
use clap::Parser;
use fp_api_client::apis::default_api::{
    proxy_create, proxy_data_sources_list, proxy_delete, proxy_get, proxy_list,
};
use fp_api_client::models::{NewProxy, ProxyConnectionStatus};
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
        Get(args) => handle_inspect_command(args).await,
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

async fn handle_list_command(args: GlobalArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let mut proxies = proxy_list(&config).await?;

    // Show connected proxies first, and then sort alphabetically by name
    proxies.sort_by(|a, b| {
        use ProxyConnectionStatus::*;
        match (a.status, b.status) {
            (Connected, Disconnected) => Ordering::Less,
            (Disconnected, Connected) => Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });

    for proxy in proxies {
        println!(
            "{} {} (ID: {}, Status: {:?})",
            match proxy.status {
                ProxyConnectionStatus::Connected => "🟢",
                ProxyConnectionStatus::Disconnected => "❌",
            },
            proxy.name,
            proxy.id,
            proxy.status
        );
    }

    Ok(())
}

async fn handle_inspect_command(args: SingleProxyArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let proxy = proxy_get(&config, &args.proxy_id).await?;
    println!(
        "Name: {}
ID: {}
Status: {:?}
Data Sources: {}",
        proxy.name,
        proxy.id,
        proxy.status,
        if proxy.data_sources.is_empty() {
            "(none)"
        } else {
            ""
        }
    );
    for data_source in proxy.data_sources {
        println!("  - {} (Type: {:?})", data_source.name, data_source._type);
    }
    Ok(())
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
