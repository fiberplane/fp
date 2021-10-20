use crate::config::api_client_configuration;
use anyhow::{anyhow, Result};
use clap::Parser;
use fiberplane_api::apis::default_api::proxy_create;
use fiberplane_api::models::NewProxy;
use petname::petname;

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

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.subcmd {
        Add(args) => handle_add_command(args).await,
    }
}

pub async fn handle_add_command(args: AddArgs) -> Result<()> {
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
