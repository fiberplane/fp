use crate::config::api_client_configuration;
use crate::interactive::{workspace_picker, workspace_user_picker};
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use anyhow::Result;
use base64uuid::Base64Uuid;
use clap::{ArgEnum, Parser};
use cli_table::Table;
use fiberplane::sorting::{SortDirection, WorkspaceInviteListingSortFields};
use fp_api_client::apis::default_api::{
    workspace_create, workspace_invite, workspace_invite_get, workspace_leave, workspace_update,
    workspace_user_remove,
};
use fp_api_client::models::{
    NewWorkspace, NewWorkspaceInvite, UpdateWorkspace, Workspace, WorkspaceInvite,
};
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

    /// List all pending invites
    ListInvites(ListInviteArgs),

    /// Leave a workspace
    Leave(LeaveArgs),

    /// Remove a user from a workspace
    Remove(RemoveArgs),

    /// Update workspace settings
    Update(UpdateArgs),
}

#[derive(ArgEnum, Clone)]
enum WorkspaceOutput {
    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,
}

#[derive(ArgEnum, Clone)]
enum PendingInvitesOutput {
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
struct ListInviteArgs {
    /// Workspace for which pending invites should be displayed
    #[clap(long, short, env)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the invites
    #[clap(long, short, default_value = "table", arg_enum)]
    output: PendingInvitesOutput,

    /// Sort the result according to the following field
    #[clap(long, arg_enum)]
    sort_by: Option<WorkspaceInviteListingSortFields>,

    /// Sort the result in the following direction
    #[clap(long, arg_enum)]
    sort_direction: Option<SortDirection>,

    /// Page to display
    #[clap(long)]
    page: Option<i32>,

    /// Amount of events to display per page
    #[clap(long)]
    limit: Option<i32>,

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

#[derive(Parser)]
struct UpdateArgs {
    /// Workspace to update settings on
    #[clap(long, short, env, global = true)]
    workspace_id: Option<Base64Uuid>,

    #[clap(subcommand)]
    sub_command: UpdateSubCommand,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
enum UpdateSubCommand {
    /// Move ownership of workspace to new owner
    Owner(MoveOwnerArgs),

    /// Change name of workspace
    Name(ChangeNameArgs),
}

#[derive(Parser)]
struct MoveOwnerArgs {
    /// ID of the member who should become workspace owner
    #[clap(long, short = 'o', env)]
    new_owner_id: Option<Base64Uuid>,

    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct ChangeNameArgs {
    /// New name for the workspace
    #[clap(long, short = 'n', env)]
    new_name: String,

    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Create(args) => handle_workspace_create(args).await,
        SubCommand::Invite(args) => handle_workspace_invite(args).await,
        SubCommand::ListInvites(args) => handle_list_invites(args).await,
        SubCommand::Leave(args) => handle_workspace_leave(args).await,
        SubCommand::Remove(args) => handle_workspace_remove_user(args).await,
        SubCommand::Update(args) => match args.sub_command {
            UpdateSubCommand::Owner(args) => handle_move_owner(args).await,
            UpdateSubCommand::Name(args) => handle_change_name(args).await,
        },
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

async fn handle_list_invites(args: ListInviteArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    let invites = workspace_invite_get(
        &config,
        &workspace_id.to_string(),
        args.sort_by.map(Into::into),
        args.sort_direction.map(Into::into),
        args.page,
        args.limit,
    )
    .await?;

    match args.output {
        PendingInvitesOutput::Table => {
            let rows: Vec<PendingInviteRow> = invites.into_iter().map(Into::into).collect();
            output_list(rows)
        }
        PendingInvitesOutput::Json => output_json(&invites),
    }
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

async fn handle_move_owner(args: MoveOwnerArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    let new_owner = workspace_user_picker(&config, &workspace_id, args.new_owner_id).await?;

    workspace_update(
        &config,
        &workspace_id.to_string(),
        UpdateWorkspace {
            title: None,
            owner: Some(new_owner.to_string()),
        },
    )
    .await?;

    info!("Successfully moved ownership of workspace");
    Ok(())
}

async fn handle_change_name(args: ChangeNameArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    workspace_update(
        &config,
        &workspace_id.to_string(),
        UpdateWorkspace {
            title: Some(args.new_name),
            owner: None,
        },
    )
    .await?;

    info!("Successfully changed name of workspace");
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

#[derive(Table)]
struct PendingInviteRow {
    #[table(title = "ID")]
    id: String,

    #[table(title = "Receiver")]
    receiver: String,

    #[table(title = "Sender")]
    sender: String,

    #[table(title = "Created at")]
    created_at: String,

    #[table(title = "Expires at")]
    expires_at: String,
}

impl From<WorkspaceInvite> for PendingInviteRow {
    fn from(invite: WorkspaceInvite) -> Self {
        Self {
            id: invite.id,
            receiver: invite
                .receiver
                .unwrap_or_else(|| "Deleted user".to_string()),
            sender: invite.sender.unwrap_or_else(|| "Deleted user".to_string()),
            created_at: invite.created_at.unwrap_or_default(),
            expires_at: invite.expires_at.unwrap_or_else(|| "Never".to_string()),
        }
    }
}
