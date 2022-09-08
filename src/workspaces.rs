use crate::config::api_client_configuration;
use crate::interactive::workspace_picker;
use anyhow::Result;
use base64uuid::Base64Uuid;
use clap::Parser;
use fp_api_client::apis::default_api::{workspace_invite, workspace_leave};
use fp_api_client::models::NewWorkspaceInvite;
use std::path::PathBuf;
use tracing::info;
use url::Url;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    /// Invite a user to a workspace
    Invite(InviteArgs),

    /// Leave a workspace
    Leave(LeaveArgs),
}

#[derive(Parser)]
struct InviteArgs {
    /// Workspace to invite the user to
    #[clap(long, short, env)]
    workspace_id: Option<Base64Uuid>,

    /// Email address of the user which should be invited
    #[clap(name = "email", required = true)]
    receiver: String,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct LeaveArgs {
    /// Workspace to leave from
    #[clap(long, short, env)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Invite(args) => handle_workspace_invite(args).await,
        SubCommand::Leave(args) => handle_workspace_leave(args).await,
    }
}

async fn handle_workspace_invite(args: InviteArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    workspace_invite(
        &config,
        &workspace_id.to_string(),
        NewWorkspaceInvite::new(args.receiver),
    )
    .await?;

    info!("Successfully invited user to workspace");
    Ok(())
}

async fn handle_workspace_leave(args: LeaveArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    workspace_leave(&config, &workspace_id.to_string()).await?;

    info!("Successfully left workspace");
    Ok(())
}
