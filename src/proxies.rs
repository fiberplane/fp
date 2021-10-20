use crate::config::api_client_configuration;
use anyhow::{anyhow, Result};
use clap::Parser;
use fiberplane_api::apis::default_api::{proxy_create, proxy_get, proxy_list};
use fiberplane_api::models::{NewProxy, ProxyConnectionStatus};
use petname::petname;
use std::cmp::Ordering;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser)]
pub enum SubCommand {
    #[clap(
        name = "add",
        about = "Add a new proxy for your organization. This returns the token the Proxy will use to authenticate with Fiberplane"
    )]
    Add(AddArgs),

    #[clap(
        name = "list",
        about = "List all proxies configured for your organization"
    )]
    List(ListArgs),
}

#[derive(Parser)]
pub struct AddArgs {
    #[clap(
        name = "name",
        about = "Proxy name (for example, you might name after different environments like production, staging, etc)"
    )]
    name: Option<String>,

    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,
}

#[derive(Parser)]
pub struct ListArgs {
    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.subcmd {
        Add(args) => handle_add_command(args).await,
        List(args) => handle_list_command(args).await,
    }
}

async fn handle_add_command(args: AddArgs) -> Result<()> {
    let name = args.name.unwrap_or_else(|| petname(2, "-"));
    let config = api_client_configuration(args.config.as_deref(), &args.base_url).await?;

    let proxy = proxy_create(&config, Some(NewProxy { name }))
        .await
        .map_err(|e| anyhow!(format!("Error adding proxy: {:?}", e)))?;

    let token = proxy
        .token
        .ok_or_else(|| anyhow!("Create proxy endpoint should have returned an API token"))?;

    println!("Added proxy \"{}\"", proxy.name);
    println!("Proxy API Token: {}", token);
    Ok(())
}

async fn handle_list_command(args: ListArgs) -> Result<()> {
    let config = api_client_configuration(args.config.as_deref(), &args.base_url).await?;
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
            "{} {} (ID: {}, status: {:?})",
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
