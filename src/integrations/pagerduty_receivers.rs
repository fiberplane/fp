use crate::config::api_client_configuration;
use crate::interactive::{name_req, workspace_picker};
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use crate::utils::clear_or_update;
use anyhow::Result;
use clap::{Parser, ValueEnum};
use cli_table::Table;
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::names::Name;
use fiberplane::models::pagerduty::{
    NewPagerDutyReceiver, PagerDutyReceiver, PagerDutyReceiverListSortFields,
    UpdatePagerDutyReceiver,
};
use fiberplane::models::sorting::SortDirection;
use petname::petname;
use std::path::PathBuf;
use time::format_description::well_known::Rfc3339;
use tracing::info;
use url::Url;

/// PagerDuty receivers allow for customization for integration with PagerDuty's
/// webhooks.
#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    /// Create a new PagerDuty receiver.
    Create(CreateArgs),

    /// Retrieve a single PagerDuty receiver.
    Get(GetArgs),

    /// Update a PagerDuty receiver.
    Update(UpdateArgs),

    /// Delete a PagerDuty receiver.
    Delete(DeleteArgs),

    /// List all PagerDuty receivers for a single workspace.
    List(ListArgs),
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Create(args) => handle_create(args).await,
        SubCommand::Get(args) => handle_get(args).await,
        SubCommand::Update(args) => handle_update(args).await,
        SubCommand::Delete(args) => handle_delete(args).await,
        SubCommand::List(args) => handle_list(args).await,
    }
}

/// A generic output for daemon related commands.
#[derive(ValueEnum, Clone)]
enum PagerDutyWebhookOutput {
    /// Output the result as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

#[derive(Parser)]
struct CreateArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// PagerDuty webhook receiver name. Use this to refer back to this
    #[clap(long, short)]
    name: Option<Name>,

    /// An optional name referencing a Template. This template will be expanded
    /// if a PagerDuty incident is created.
    #[clap(long, short)]
    incident_created_template: Option<Name>,

    /// An optional secret to set on the PagerDuty receiver. If this is set,
    /// then any incoming webhook will be verified against this secret.
    #[clap(long)]
    secret: Option<String>,

    /// Output of the webhooks
    #[clap(long, short, default_value = "table", value_enum)]
    output: PagerDutyWebhookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    #[clap(from_global)]
    token: Option<String>,
}

async fn handle_create(args: CreateArgs) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let default_name = Name::new(petname(2, "-")).expect("petname should be valid name");
    let name = name_req(
        "PagerDuty webhook receiver name",
        args.name,
        Some(default_name),
    )?;

    let new_pagerduty_receiver = NewPagerDutyReceiver::builder()
        .incident_created_template_name(args.incident_created_template)
        .secret(args.secret)
        .build();
    let pagerduty_receiver = client
        .pagerduty_receiver_create(workspace_id, &name, new_pagerduty_receiver)
        .await?;

    match args.output {
        PagerDutyWebhookOutput::Table => {
            output_details(GenericKeyValue::from_pagerduty_receiver(pagerduty_receiver))
        }
        PagerDutyWebhookOutput::Json => output_json(&pagerduty_receiver),
    }
}

#[derive(Parser)]
struct GetArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// PagerDuty webhook receiver name.
    #[clap(long, short)]
    name: Option<Name>,

    /// Output of the webhooks
    #[clap(long, short, default_value = "table", value_enum)]
    output: PagerDutyWebhookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    #[clap(from_global)]
    token: Option<String>,
}

async fn handle_get(args: GetArgs) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let name = name_req("PagerDuty webhook receiver name", args.name, None)?;

    let pagerduty_receiver = client.pagerduty_receiver_get(workspace_id, &name).await?;

    match args.output {
        PagerDutyWebhookOutput::Table => {
            output_details(GenericKeyValue::from_pagerduty_receiver(pagerduty_receiver))
        }
        PagerDutyWebhookOutput::Json => output_json(&pagerduty_receiver),
    }
}

#[derive(Parser)]
struct UpdateArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// PagerDuty webhook receiver name.
    #[clap(long, short)]
    name: Option<Name>,

    /// An optional name referencing a Template. This template will be expanded
    /// if a PagerDuty incident is created.
    #[clap(long, env, conflicts_with = "clear_incident_creation_template")]
    incident_creation_template: Option<Name>,

    /// Clear the incident creation template reference.
    #[clap(long, env, conflicts_with = "incident_creation_template")]
    clear_incident_creation_template: bool,

    /// An optional secret to set on the PagerDuty receiver. If this is set,
    /// then any incoming webhook will be verified against this secret
    #[clap(long, env, conflicts_with = "clear_secret")]
    secret: Option<String>,

    /// Clear the secret on the PagerDuty receiver.
    #[clap(long, env, conflicts_with = "secret")]
    clear_secret: bool,

    /// Output of the webhooks
    #[clap(long, short, default_value = "table", value_enum)]
    output: PagerDutyWebhookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    #[clap(from_global)]
    token: Option<String>,
}

async fn handle_update(args: UpdateArgs) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let default_name = Name::new(petname(2, "-")).expect("petname should be valid name");
    let name = name_req(
        "PagerDuty webhook receiver name",
        args.name,
        Some(default_name),
    )?;

    let incident_created_template_name = clear_or_update(
        args.clear_incident_creation_template,
        args.incident_creation_template,
    );

    let secret = clear_or_update(args.clear_secret, args.secret);

    let update_pagerduty_receiver = UpdatePagerDutyReceiver::builder()
        .incident_created_template_name(incident_created_template_name)
        .secret(secret)
        .build();

    let pagerduty_receiver = client
        .pagerduty_receiver_update(workspace_id, &name, update_pagerduty_receiver)
        .await?;

    match args.output {
        PagerDutyWebhookOutput::Table => {
            output_details(GenericKeyValue::from_pagerduty_receiver(pagerduty_receiver))
        }
        PagerDutyWebhookOutput::Json => output_json(&pagerduty_receiver),
    }
}

#[derive(Parser)]
struct DeleteArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// PagerDuty webhook receiver name.
    #[clap(long, short)]
    name: Option<Name>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    #[clap(from_global)]
    token: Option<String>,
}

async fn handle_delete(args: DeleteArgs) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let name = name_req("PagerDuty webhook receiver name", args.name, None)?;

    client
        .pagerduty_receiver_delete(workspace_id, &name)
        .await?;

    info!("Deleted Pagerduty receiver");
    Ok(())
}

#[derive(Parser)]
struct ListArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Sort the result according to the following field
    #[clap(long, value_enum)]
    sort_by: Option<PagerDutyReceiverListSortFields>,

    /// Sort the result in the following direction
    #[clap(long, value_enum)]
    sort_direction: Option<SortDirection>,

    /// Page to display
    #[clap(long)]
    page: Option<i32>,

    /// Amount of integrations to display per page
    #[clap(long)]
    limit: Option<i32>,

    /// Output of the webhooks
    #[clap(long, short, default_value = "table", value_enum)]
    output: PagerDutyWebhookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    #[clap(from_global)]
    token: Option<String>,
}

async fn handle_list(args: ListArgs) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;

    let pagerduty_receivers = client
        .pagerduty_receiver_list(
            workspace_id,
            args.page,
            args.limit,
            args.sort_by.map(Into::into),
            args.sort_direction.map(Into::into),
        )
        .await?;

    if pagerduty_receivers.has_more_results {
        info!(total_results = pagerduty_receivers.total_results, "There are more results available. Please use the --page and --limit flags to paginate through the results.")
    }

    match args.output {
        PagerDutyWebhookOutput::Table => {
            let pagerduty_receivers: Vec<PagerDutyReceiverRow> =
                pagerduty_receivers.into_iter().map(Into::into).collect();
            output_list(pagerduty_receivers)
        }
        PagerDutyWebhookOutput::Json => output_json(&pagerduty_receivers),
    }
}

#[derive(Table)]
pub struct PagerDutyReceiverRow {
    #[table(title = "Name")]
    pub name: String,

    #[table(title = "Incident Created Template")]
    pub incident_created_template: String,

    #[table(title = "Updated at")]
    pub updated_at: String,

    #[table(title = "Created at")]
    pub created_at: String,
}

impl From<PagerDutyReceiver> for PagerDutyReceiverRow {
    fn from(pagerduty_receiver: PagerDutyReceiver) -> Self {
        Self {
            name: pagerduty_receiver.name.to_string(),
            incident_created_template: pagerduty_receiver
                .incident_created_template_name
                .map(|name| name.to_string())
                .unwrap_or_else(|| String::from("<none>")),
            updated_at: pagerduty_receiver
                .updated_at
                .format(&Rfc3339)
                .unwrap_or_default(),
            created_at: pagerduty_receiver
                .created_at
                .format(&Rfc3339)
                .unwrap_or_default(),
        }
    }
}

impl GenericKeyValue {
    pub fn from_pagerduty_receiver(pagerduty_receiver: PagerDutyReceiver) -> Vec<GenericKeyValue> {
        vec![
            GenericKeyValue::new("Name:", pagerduty_receiver.name),
            GenericKeyValue::new(
                "Incident created template:",
                pagerduty_receiver
                    .incident_created_template_name
                    .map(|name| name.to_string())
                    .unwrap_or_else(|| String::from("<none>")),
            ),
            GenericKeyValue::new("Webhook URL:", pagerduty_receiver.webhook_url),
            GenericKeyValue::new("Secret set:", pagerduty_receiver.secret_set.to_string()),
            GenericKeyValue::new(
                "Created at:",
                pagerduty_receiver
                    .created_at
                    .format(&Rfc3339)
                    .unwrap_or_default(),
            ),
            GenericKeyValue::new(
                "Updated at:",
                pagerduty_receiver
                    .updated_at
                    .format(&Rfc3339)
                    .unwrap_or_default(),
            ),
        ]
    }
}
