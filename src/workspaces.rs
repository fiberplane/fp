use crate::config::api_client_configuration;
use crate::interactive::{
    data_source_picker, default_theme, workspace_picker, workspace_user_picker,
};
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use anyhow::{bail, Result};
use base64uuid::Base64Uuid;
use clap::{ArgEnum, Parser};
use cli_table::Table;
use dialoguer::FuzzySelect;
use fiberplane::sorting::{
    SortDirection, WorkspaceInviteListingSortFields, WorkspaceListingSortFields,
};
use fp_api_client::apis::default_api::{
    workspace_create, workspace_get, workspace_invite, workspace_invite_get, workspace_leave,
    workspace_list, workspace_update, workspace_user_remove,
};
use fp_api_client::models::{
    NewWorkspace, NewWorkspaceInvite, SelectedDataSource, UpdateWorkspace, Workspace,
    WorkspaceInvite, WorkspaceInviteResponse,
};
use std::collections::HashMap;
use std::fmt::Display;
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

    /// List all workspaces of which you're a member
    List(ListArgs),

    /// List all pending invites
    ListInvites(ListInviteArgs),

    /// Leave a workspace
    Leave(LeaveArgs),

    /// Remove a user from a workspace
    Remove(RemoveArgs),

    /// Update workspace settings
    #[clap(subcommand)]
    Update(UpdateSubCommand),
}

#[derive(ArgEnum, Clone)]
enum WorkspaceOutput {
    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,
}

#[derive(ArgEnum, Clone)]
enum NewInviteOutput {
    /// Output the details as plain text
    InviteUrl,

    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,
}

#[derive(ArgEnum, Clone)]
enum WorkspaceListOutput {
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

    /// Output of the invite
    #[clap(long, short, default_value = "table", arg_enum)]
    output: NewInviteOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct ListArgs {
    /// Output of the workspaces
    #[clap(long, short, default_value = "table", arg_enum)]
    output: WorkspaceListOutput,

    /// Sort the result according to the following field
    #[clap(long, arg_enum)]
    sort_by: Option<WorkspaceListingSortFields>,

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
enum UpdateSubCommand {
    /// Move ownership of workspace to new owner
    Owner(MoveOwnerArgs),

    /// Change name of workspace
    Name(ChangeNameArgs),

    /// Change the default data sources
    #[clap(subcommand)]
    DefaultDataSources(UpdateDefaultDataSourcesSubCommand),
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

#[derive(Parser)]
enum UpdateDefaultDataSourcesSubCommand {
    /// Set the default data source for the given provider type
    Set(SetDefaultDataSourcesArgs),

    /// Unset the default data source for the given provider type
    Unset(UnsetDefaultDataSourcesArgs),
}

#[derive(Parser)]
struct SetDefaultDataSourcesArgs {
    /// Name of the data source which should be set as default for the given provider type
    #[clap(long, short, env)]
    data_source_name: Option<String>,

    /// If the data source is a proxy data source, the name of the proxy
    #[clap(long, short, env)]
    proxy_name: Option<String>,

    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct UnsetDefaultDataSourcesArgs {
    /// Provider type for which the default data source should be unset
    #[clap(long, short, env)]
    provider_type: Option<String>,

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
        SubCommand::List(args) => handle_workspace_list(args).await,
        SubCommand::ListInvites(args) => handle_list_invites(args).await,
        SubCommand::Leave(args) => handle_workspace_leave(args).await,
        SubCommand::Remove(args) => handle_workspace_remove_user(args).await,
        SubCommand::Update(sub_command) => match sub_command {
            UpdateSubCommand::Owner(args) => handle_move_owner(args).await,
            UpdateSubCommand::Name(args) => handle_change_name(args).await,
            UpdateSubCommand::DefaultDataSources(sub_command) => match sub_command {
                UpdateDefaultDataSourcesSubCommand::Set(args) => {
                    handle_set_default_data_source(args).await
                }
                UpdateDefaultDataSourcesSubCommand::Unset(args) => {
                    handle_unset_default_data_source(args).await
                }
            },
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

    let invite = workspace_invite(
        &config,
        &workspace_id.to_string(),
        NewWorkspaceInvite::new(args.receiver),
    )
    .await?;

    if !matches!(args.output, NewInviteOutput::InviteUrl) {
        info!("Successfully invited user to workspace");
    }

    match args.output {
        NewInviteOutput::InviteUrl => {
            println!("{}", invite.url);
            Ok(())
        }
        NewInviteOutput::Table => output_details(GenericKeyValue::from_invite_response(invite)),
        NewInviteOutput::Json => output_json(&invite),
    }
}

async fn handle_workspace_list(args: ListArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let list = workspace_list(
        &config,
        args.sort_by.map(Into::into),
        args.sort_direction.map(Into::into),
    )
    .await?;

    match args.output {
        WorkspaceListOutput::Table => {
            let rows: Vec<WorkspaceRow> = list.into_iter().map(Into::into).collect();
            output_list(rows)
        }
        WorkspaceListOutput::Json => output_json(&list),
    }
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
            owner: Some(new_owner.to_string()),
            name: None,
            default_data_sources: None,
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
            name: Some(args.new_name),
            owner: None,
            default_data_sources: None,
        },
    )
    .await?;

    info!("Successfully changed name of workspace");
    Ok(())
}

async fn handle_set_default_data_source(args: SetDefaultDataSourcesArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    let data_source =
        data_source_picker(&config, Some(workspace_id), args.data_source_name).await?;

    let mut default_data_sources = workspace_get(&config, &workspace_id.to_string())
        .await?
        .default_data_sources;
    default_data_sources.insert(
        data_source.provider_type,
        SelectedDataSource {
            name: data_source.name,
            proxy_name: data_source.proxy_name,
        },
    );

    workspace_update(
        &config,
        &workspace_id.to_string(),
        UpdateWorkspace {
            default_data_sources: Some(default_data_sources),
            name: None,
            owner: None,
        },
    )
    .await?;

    info!("Successfully set default data source for workspace");
    Ok(())
}

async fn handle_unset_default_data_source(args: UnsetDefaultDataSourcesArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    let mut default_data_sources = workspace_get(&config, &workspace_id.to_string())
        .await?
        .default_data_sources;

    let mut provider_types: Vec<String> = default_data_sources.keys().cloned().collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt("Provider type")
        .items(&provider_types)
        .default(0)
        .interact_opt()?;

    let provider_type = match selection {
        Some(selection) => provider_types.remove(selection),
        None => bail!("No data source selected"),
    };

    default_data_sources.remove(&provider_type);

    workspace_update(
        &config,
        &workspace_id.to_string(),
        UpdateWorkspace {
            default_data_sources: Some(default_data_sources),
            name: None,
            owner: None,
        },
    )
    .await?;

    info!("Successfully unset default data source for workspace");
    Ok(())
}

impl GenericKeyValue {
    fn from_workspace(workspace: Workspace) -> Vec<Self> {
        vec![
            GenericKeyValue::new("Name:", workspace.name),
            GenericKeyValue::new("Type:", format!("{:?}", workspace._type)),
            GenericKeyValue::new("ID:", workspace.id),
            GenericKeyValue::new(
                "Default Data Sources:",
                workspace
                    .default_data_sources
                    .iter()
                    .map(|(name, data_source)| {
                        format!(
                            "{} -> {}{}",
                            name,
                            data_source.name,
                            if let Some(proxy_name) = &data_source.proxy_name {
                                format!(" (Proxy: {})", proxy_name)
                            } else {
                                "".to_string()
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        ]
    }

    fn from_invite_response(response: WorkspaceInviteResponse) -> Vec<Self> {
        vec![GenericKeyValue::new("URL:", response.url)]
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

#[derive(Table)]
struct WorkspaceRow {
    #[table(title = "ID")]
    pub id: String,

    #[table(title = "Name")]
    pub name: String,

    #[table(title = "Type")]
    pub _type: String,

    #[table(title = "Default Data Sources", display_fn = "print_data_sources")]
    pub default_data_sources: HashMap<String, SelectedDataSource>,

    #[table(title = "Created at")]
    pub created_at: String,

    #[table(title = "Updated at")]
    pub updated_at: String,
}

impl From<Workspace> for WorkspaceRow {
    fn from(workspace: Workspace) -> Self {
        Self {
            id: workspace.id,
            name: workspace.name,
            _type: format!("{:?}", workspace._type),
            default_data_sources: workspace.default_data_sources,
            created_at: workspace.created_at,
            updated_at: workspace.updated_at,
        }
    }
}

fn print_data_sources(input: &HashMap<String, SelectedDataSource>) -> impl Display {
    let mut output = String::new();
    let mut iterator = input.iter().peekable();

    while let Some((key, value)) = iterator.next() {
        output.push_str(key);

        if let Some(proxy_name) = &value.proxy_name {
            output.push('=');
            output.push_str(proxy_name);
        }

        if iterator.peek().is_some() {
            output.push_str(", ");
        }
    }

    output
}
