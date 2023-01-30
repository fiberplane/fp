use crate::config::api_client_configuration;
use crate::interactive::{notebook_picker, workspace_picker};
use anyhow::Result;
use clap::Parser;
use fiberplane::api_client::{front_matter_delete, front_matter_update};
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::notebooks::FrontMatter;
use std::path::PathBuf;
use tracing::info;
use url::Url;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Update(args) => handle_front_matter_update_command(args).await,
        Clear(args) => handle_front_matter_clear_command(args).await,
    }
}

#[derive(Parser)]
enum SubCommand {
    /// Updates front matter for an existing notebook
    Update(UpdateArguments),

    /// Clears all front matter from an existing notebook
    Clear(ClearArguments),
}

#[derive(Parser)]
struct UpdateArguments {
    /// Front matter which should be added. Can override existing keys.
    /// To delete an existing key, set its value to `null`
    #[clap(value_parser = parse_from_str)]
    front_matter: FrontMatter,

    /// Notebook for which front matter should be updated for
    #[clap(long)]
    notebook_id: Option<Base64Uuid>,

    /// Workspace in which the notebook resides in
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_front_matter_update_command(args: UpdateArguments) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let notebook_id = notebook_picker(&client, args.notebook_id, Some(workspace_id)).await?;

    front_matter_update(&client, notebook_id, args.front_matter).await?;

    info!("Successfully updated front matter");
    Ok(())
}

#[derive(Parser)]
struct ClearArguments {
    /// Notebook for which front matter should be cleared for
    #[clap(long)]
    notebook_id: Option<Base64Uuid>,

    /// Workspace in which the notebook resides in
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_front_matter_clear_command(args: ClearArguments) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let notebook_id = notebook_picker(&client, args.notebook_id, Some(workspace_id)).await?;

    front_matter_delete(&client, notebook_id).await?;

    info!("Successfully cleared front matter");
    Ok(())
}

pub fn parse_from_str(input: &str) -> serde_json::Result<FrontMatter> {
    serde_json::from_str(input)
}
