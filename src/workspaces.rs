use crate::config::api_client_configuration;
use crate::interactive::{workspace_picker, workspace_user_picker};
use crate::output::{output_details, output_json, GenericKeyValue};
use anyhow::Result;
use base64uuid::Base64Uuid;
use clap::{ArgEnum, Parser};
use fp_api_client::apis::default_api::{
    workspace_create, workspace_invite, workspace_leave, workspace_user_remove,
};
use fp_api_client::models::{NewWorkspace, NewWorkspaceInvite, Workspace};
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
    /// Create a new workspace
    Create(CreateArgs),

    /// Invite a user to a workspace
    Invite(InviteArgs),

    /// Leave a workspace
    Leave(LeaveArgs),

    /// Remove a user from a workspace
    Remove(RemoveArgs),
}

#[derive(ArgEnum, Clone)]
enum WorkspaceOutput {
    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,
}

#[derive(Parser)]
struct CreateArgs {
    /// Name of the new workspace
    #[clap(short, long)]
    name: String,

    /// Output of the workspace
    #[clap(long, short, default_value = "table", arg_enum)]
    output: WorkspaceOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
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

#[derive(Parser)]
struct RemoveArgs {
    /// Workspace to remove the user from
    #[clap(long, short, env)]
    workspace_id: Option<Base64Uuid>,

    /// User ID of the user that should be removed from the workspace
    #[clap(long, short, env)]
    user_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Create(args) => handle_workspace_create(args).await,
        SubCommand::Invite(args) => handle_workspace_invite(args).await,
        SubCommand::Leave(args) => handle_workspace_leave(args).await,
        SubCommand::Remove(args) => handle_workspace_remove_user(args).await,
    }
}

async fn handle_workspace_create(args: CreateArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let workspace = workspace_create(&config, NewWorkspace::new(args.name)).await?;

    info!("Successfully created new workspace");

    match args.output {
        WorkspaceOutput::Table => output_details(GenericKeyValue::from_workspace(workspace)),
        WorkspaceOutput::Json => output_json(&workspace),
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

async fn handle_workspace_remove_user(args: RemoveArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let workspace_id = workspace_picker(&config, args.workspace_id).await?;
    let user = workspace_user_picker(&config, &workspace_id, args.user_id).await?;

    workspace_user_remove(&config, &workspace_id.to_string(), &user.to_string()).await?;

    info!("Successfully removed user from workspace");
    Ok(())
}

impl GenericKeyValue {
    fn from_workspace(workspace: Workspace) -> Vec<Self> {
        vec![
            GenericKeyValue::new("Name:", workspace.name),
            GenericKeyValue::new("Type:", format!("{:?}", workspace._type)),
            GenericKeyValue::new("ID:", workspace.id),
        ]
    }
}
