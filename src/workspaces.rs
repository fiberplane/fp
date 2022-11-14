use crate::config::api_client_configuration;
use crate::interactive::{
    data_source_picker, default_theme, text_req, workspace_picker, workspace_user_picker,
};
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use anyhow::{bail, Result};
use base64uuid::Base64Uuid;
use clap::{Parser, ValueEnum};
use cli_table::Table;
use dialoguer::FuzzySelect;
use fiberplane::protocols::core::AuthRole;
use fiberplane::sorting::{
    SortDirection, WorkspaceInviteListingSortFields, WorkspaceListingSortFields,
    WorkspaceMembershipSortFields,
};
use fp_api_client::apis::default_api::{
    workspace_create, workspace_delete, workspace_get, workspace_invite, workspace_invite_delete,
    workspace_invite_get, workspace_leave, workspace_list, workspace_update, workspace_user_remove,
    workspace_user_update, workspace_users_list,
};
use fp_api_client::models::{
    Membership, NewWorkspace, NewWorkspaceInvite, SelectedDataSource, UpdateWorkspace, Workspace,
    WorkspaceInvite, WorkspaceInviteResponse, WorkspaceUserUpdate,
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

    /// Delete a workspace
    Delete(DeleteArgs),

    /// Create, list and delete invites for a workspace
    #[clap(subcommand)]
    Invites(InvitesSubCommand),

    /// List all workspaces of which you're a member
    List(ListArgs),

    /// Leave a workspace
    Leave(LeaveArgs),

    /// Update workspace settings
    #[clap(subcommand)]
    Settings(SettingsSubCommand),

    /// List, update and remove users from a workspace
    #[clap(subcommand)]
    Users(UsersSubCommand),
}

#[derive(Parser)]
enum InvitesSubCommand {
    /// Create a new invitation to join a workspace
    #[clap(aliases = &["invite"])]
    Create(InviteCreateArgs),

    /// List all pending invites for a workspace
    List(InviteListArgs),

    /// Delete a pending invite from a workspace
    #[clap(aliases = &["remove", "rm"])]
    Delete(InviteDeleteArgs),
}

#[derive(Parser)]
enum UsersSubCommand {
    /// List the users that part of a workspace
    List(UserListArgs),

    /// Update the user within a workspace
    Update(UserUpdateArgs),

    /// Delete a user from a workspace
    #[clap(aliases = &["remove", "rm"])]
    Delete(UserDeleteArgs),
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Create(args) => handle_workspace_create(args).await,
        SubCommand::Delete(args) => handle_workspace_delete(args).await,
        SubCommand::List(args) => handle_workspace_list(args).await,
        SubCommand::Leave(args) => handle_workspace_leave(args).await,
        SubCommand::Invites(sub_command) => match sub_command {
            InvitesSubCommand::Create(args) => handle_invite_create(args).await,
            InvitesSubCommand::List(args) => handle_invite_list(args).await,
            InvitesSubCommand::Delete(args) => handle_invite_delete(args).await,
        },
        SubCommand::Users(sub_command) => match sub_command {
            UsersSubCommand::List(args) => handle_user_list(args).await,
            UsersSubCommand::Update(args) => handle_user_update(args).await,
            UsersSubCommand::Delete(args) => handle_user_delete(args).await,
        },
        SubCommand::Settings(sub_command) => match sub_command {
            SettingsSubCommand::Owner(args) => handle_move_owner(args).await,
            SettingsSubCommand::Name(args) => handle_change_name(args).await,
            SettingsSubCommand::DefaultDataSources(sub_command) => {
                handle_default_data_sources_command(sub_command).await
            }
        },
    }
}

pub(crate) async fn handle_default_data_sources_command(
    sub_command: DefaultDataSourcesSubCommand,
) -> Result<()> {
    match sub_command {
        DefaultDataSourcesSubCommand::Get(args) => handle_get_default_data_sources(args).await,
        DefaultDataSourcesSubCommand::Set(args) => handle_set_default_data_source(args).await,
        DefaultDataSourcesSubCommand::Unset(args) => handle_unset_default_data_source(args).await,
    }
}

#[derive(Parser)]
struct CreateArgs {
    /// Name of the new workspace
    #[clap(short, long)]
    name: Option<String>,

    /// Output of the workspace
    #[clap(long, short, default_value = "table", value_enum)]
    output: WorkspaceOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_workspace_create(args: CreateArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let name = text_req("Name", args.name, None)?;

    let workspace = workspace_create(&config, NewWorkspace::new(name)).await?;

    info!("Successfully created new workspace");

    match args.output {
        WorkspaceOutput::Table => output_details(GenericKeyValue::from_workspace(workspace)),
        WorkspaceOutput::Json => output_json(&workspace),
    }
}

#[derive(Parser)]
struct DeleteArgs {
    /// Workspace to delete
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_workspace_delete(args: DeleteArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    workspace_delete(&config, &workspace_id.to_string()).await?;

    info!("Successfully deleted workspace");
    Ok(())
}

#[derive(Parser)]
struct ListArgs {
    /// Output of the workspaces
    #[clap(long, short, default_value = "table", value_enum)]
    output: WorkspaceListOutput,

    /// Sort the result according to the following field
    #[clap(long, value_enum)]
    sort_by: Option<WorkspaceListingSortFields>,

    /// Sort the result in the following direction
    #[clap(long, value_enum)]
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

#[derive(Parser)]
struct LeaveArgs {
    /// Workspace to leave from
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_workspace_leave(args: LeaveArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    workspace_leave(&config, &workspace_id.to_string()).await?;

    info!("Successfully left workspace");
    Ok(())
}

#[derive(Parser)]
struct InviteCreateArgs {
    /// Workspace to invite the user to
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Email address of the user which should be invited
    #[clap(name = "email", required = true)]
    email: Option<String>,

    /// Role which the invited user should receive upon accepting the invite
    #[clap(name = "role", default_value = "write", value_enum)]
    role: AuthRole,

    /// Output of the invite
    #[clap(long, short, default_value = "table", value_enum)]
    output: NewInviteOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_invite_create(args: InviteCreateArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    let email = text_req("Email", args.email, None)?;

    let role = match args.role {
        AuthRole::Read => fp_api_client::models::AuthRole::Read,
        AuthRole::Write => fp_api_client::models::AuthRole::Write,
        AuthRole::Admin => fp_api_client::models::AuthRole::Admin,
    };

    let invite = workspace_invite(
        &config,
        &workspace_id.to_string(),
        NewWorkspaceInvite::new(email, role),
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

#[derive(Parser)]
struct InviteListArgs {
    /// Workspace for which pending invites should be displayed
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the invites
    #[clap(long, short, default_value = "table", value_enum)]
    output: PendingInvitesOutput,

    /// Sort the result according to the following field
    #[clap(long, value_enum)]
    sort_by: Option<WorkspaceInviteListingSortFields>,

    /// Sort the result in the following direction
    #[clap(long, value_enum)]
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

async fn handle_invite_list(args: InviteListArgs) -> Result<()> {
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

#[derive(Parser)]
struct InviteDeleteArgs {
    /// Invitation ID to delete
    #[clap(long, short, env)]
    invite_id: Base64Uuid,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_invite_delete(args: InviteDeleteArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    workspace_invite_delete(&config, &args.invite_id.to_string()).await?;

    info!("Successfully deleted invitation from workspace");
    Ok(())
}

#[derive(Parser)]
struct UserListArgs {
    /// Workspace for which pending invites should be displayed
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the invites
    #[clap(long, short, default_value = "table", value_enum)]
    output: UserListOutput,

    /// Sort the result according to the following field
    #[clap(long, value_enum)]
    sort_by: Option<WorkspaceMembershipSortFields>,

    /// Sort the result in the following direction
    #[clap(long, value_enum)]
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

async fn handle_user_list(args: UserListArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    let users = workspace_users_list(
        &config,
        &workspace_id.to_string(),
        args.sort_by.map(Into::into),
        args.sort_direction.map(Into::into),
    )
    .await?;

    match args.output {
        UserListOutput::Table => {
            let rows: Vec<MembershipRow> = users.into_iter().map(Into::into).collect();
            output_list(rows)
        }
        UserListOutput::Json => output_json(&users),
    }
}

#[derive(Parser)]
struct UserUpdateArgs {
    /// New role which should be assigned to the specified user
    #[clap(long, value_enum)]
    role: Option<AuthRole>,

    /// Workspace to update the user in
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// User ID of the user that should be updated within the workspace
    #[clap(long, short, env)]
    user_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_user_update(args: UserUpdateArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let workspace_id = workspace_picker(&config, args.workspace_id).await?;
    let user = workspace_user_picker(&config, &workspace_id, args.user_id).await?;

    workspace_user_update(
        &config,
        &workspace_id.to_string(),
        &user.to_string(),
        WorkspaceUserUpdate {
            // Once we have our own openapi client implemented this will be literally just `args.role` (without the `.map`)
            role: args.role.map(|role| match role {
                AuthRole::Read => fp_api_client::models::AuthRole::Read,
                AuthRole::Write => fp_api_client::models::AuthRole::Write,
                AuthRole::Admin => fp_api_client::models::AuthRole::Admin,
            }),
        },
    )
    .await?;

    info!("Successfully updated user within workspace");
    Ok(())
}

#[derive(Parser)]
struct UserDeleteArgs {
    /// Workspace to remove the user from
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// User ID of the user that should be removed from the workspace
    #[clap(long, short, env)]
    user_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_user_delete(args: UserDeleteArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let workspace_id = workspace_picker(&config, args.workspace_id).await?;
    let user = workspace_user_picker(&config, &workspace_id, args.user_id).await?;

    workspace_user_remove(&config, &workspace_id.to_string(), &user.to_string()).await?;

    info!("Successfully removed user from workspace");
    Ok(())
}

#[derive(Parser)]
enum SettingsSubCommand {
    /// Move ownership of workspace to new owner
    Owner(MoveOwnerArgs),

    /// Change name of workspace
    Name(ChangeNameArgs),

    /// Change the default data sources
    #[clap(subcommand)]
    DefaultDataSources(DefaultDataSourcesSubCommand),
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
pub(crate) enum DefaultDataSourcesSubCommand {
    /// Get the default data sources
    Get(GetDefaultDataSourcesArgs),

    /// Set the default data source for the given provider type
    Set(SetDefaultDataSourcesArgs),

    /// Unset the default data source for the given provider type
    Unset(UnsetDefaultDataSourcesArgs),
}

#[derive(Parser)]
pub(crate) struct GetDefaultDataSourcesArgs {
    /// Display format for the output
    #[clap(long, short, default_value = "table", value_enum)]
    output: WorkspaceOutput,

    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub(crate) struct SetDefaultDataSourcesArgs {
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
pub(crate) struct UnsetDefaultDataSourcesArgs {
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

async fn handle_get_default_data_sources(args: GetDefaultDataSourcesArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    let default_data_sources = workspace_get(&config, &workspace_id.to_string())
        .await?
        .default_data_sources;

    match args.output {
        WorkspaceOutput::Table => {
            let table: Vec<SelectedDataSourceRow> =
                default_data_sources.into_iter().map(Into::into).collect();
            output_list(table)
        }
        WorkspaceOutput::Json => output_json(&default_data_sources),
    }
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
        data_source.provider_type.clone(),
        SelectedDataSource {
            name: data_source.name.clone(),
            proxy_name: data_source.proxy_name.clone(),
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

    info!(
        "Successfully set {}{} to be the default data source for {} queries",
        data_source.name,
        if let Some(proxy) = data_source.proxy_name {
            format!(" (proxy: {})", proxy)
        } else {
            String::new()
        },
        data_source.provider_type
    );
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

#[derive(ValueEnum, Clone)]
enum WorkspaceOutput {
    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,
}

#[derive(ValueEnum, Clone)]
enum NewInviteOutput {
    /// Output the details as plain text
    InviteUrl,

    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,
}

#[derive(ValueEnum, Clone)]
enum WorkspaceListOutput {
    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,
}

#[derive(ValueEnum, Clone)]
enum PendingInvitesOutput {
    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,
}

#[derive(ValueEnum, Clone)]
enum UserListOutput {
    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,
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

#[derive(Table)]
struct SelectedDataSourceRow {
    #[table(title = "Provider Type")]
    pub provider_type: String,

    #[table(title = "Data Source Name")]
    pub name: String,

    #[table(title = "Proxy Name")]
    pub proxy_name: String,
}

impl From<(String, SelectedDataSource)> for SelectedDataSourceRow {
    fn from(selected: (String, SelectedDataSource)) -> Self {
        Self {
            provider_type: selected.0,
            name: selected.1.name,
            proxy_name: selected.1.proxy_name.unwrap_or_default(),
        }
    }
}

#[derive(Table)]
struct MembershipRow {
    #[table(title = "ID")]
    pub id: String,

    #[table(title = "Name")]
    pub name: String,

    #[table(title = "Email")]
    pub email: String,

    #[table(title = "Role")]
    pub role: String,
}

impl From<Membership> for MembershipRow {
    fn from(user: Membership) -> Self {
        Self {
            id: user.id,
            name: user.name,
            email: user.email.unwrap_or_else(|| "unknown".to_string()),
            role: user.role.to_string(),
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
