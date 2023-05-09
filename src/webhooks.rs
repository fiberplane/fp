use crate::config::api_client_configuration;
use crate::interactive::{
    bool_req, confirm, text_opt, webhook_category_picker, webhook_delivery_picker, webhook_picker,
    workspace_picker,
};
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use anyhow::Result;
use clap::Parser;
use clap::ValueEnum;
use cli_table::Table;
use fiberplane::api_client::{
    webhook_create, webhook_delete, webhook_delivery_get, webhook_delivery_list,
    webhook_delivery_resend, webhook_update, webhooks_list,
};
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::webhooks::{
    NewWebhook, UpdateWebhook, Webhook, WebhookCategory, WebhookDelivery, WebhookDeliverySummary,
};
use std::path::PathBuf;
use time::format_description::well_known::Rfc3339;
use tracing::{info, warn};
use url::Url;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    /// Create a new webhook
    Create(CreateArgs),

    /// List all webhooks
    List(ListArgs),

    /// Delete a webhook
    Delete(DeleteArgs),

    /// Update a webhook
    Update(UpdateArgs),

    /// View webhook deliveries and optionally resend them, if they errored
    #[clap(subcommand)]
    Deliveries(DeliveriesSubCommand),
}

#[derive(Parser)]
enum DeliveriesSubCommand {
    /// List all deliveries
    List(WebhookDeliveryListArgs),

    /// Get detailed information about a delivery
    Info(WebhookDeliveryInfoArgs),

    /// Resend a delivery
    Resend(WebhookDeliveryResendArgs),
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Create(args) => handle_webhook_create(args).await,
        SubCommand::List(args) => handle_webhook_list(args).await,
        SubCommand::Delete(args) => handle_webhook_delete(args).await,
        SubCommand::Update(args) => handle_webhook_update(args).await,
        SubCommand::Deliveries(command) => match command {
            DeliveriesSubCommand::List(args) => handle_webhook_delivery_list(args).await,
            DeliveriesSubCommand::Info(args) => handle_webhook_delivery_info(args).await,
            DeliveriesSubCommand::Resend(args) => handle_webhook_delivery_resend(args).await,
        },
    }
}

#[derive(Parser)]
struct CreateArgs {
    /// List of categories which this new webhook should receive deliveries for
    #[clap(long, value_enum)]
    categories: Option<Vec<WebhookCategory>>,

    /// Endpoint URL to which deliveries should be sent to.
    /// Must start with `http` or `https`
    #[clap(long)]
    endpoint: Option<String>,

    /// Whenever the newly created webhook should be enabled
    #[clap(long)]
    enabled: Option<bool>,

    /// Workspace for which this webhook receives deliveries
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the webhook
    #[clap(long, short, default_value = "table", value_enum)]
    output: WebhookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_webhook_create(args: CreateArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let categories = webhook_category_picker(args.categories)?;
    let endpoint = text_opt("Endpoint Url", args.endpoint, None).unwrap();
    let enabled = bool_req("Enabled", args.enabled, true);

    let payload = NewWebhook::builder()
        .events(categories)
        .endpoint(endpoint)
        .enabled(enabled)
        .build();

    let webhook = webhook_create(&client, workspace_id, payload).await?;

    if !webhook.successful {
        warn!("The webhook has been created in the disabled state because it failed to handle the \"ping\" event.");
        warn!(
            "See documentation at https://docs.fiberplane.com/docs/webhooks for more information."
        );
    } else {
        info!("Successfully created new webhook.");
    }

    info!("Don't forget to copy the shared secret, as this will be the last time you'll see it.");

    match args.output {
        WebhookOutput::Table => output_details(GenericKeyValue::from_webhook(webhook)),
        WebhookOutput::Json => output_json(&webhook),
    }
}

#[derive(Parser)]
struct ListArgs {
    /// Page to display
    #[clap(long)]
    page: Option<i32>,

    /// Amount of webhooks to display per page
    #[clap(long)]
    limit: Option<i32>,

    /// Workspace for which webhooks should be listed
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the webhooks
    #[clap(long, short, default_value = "table", value_enum)]
    output: WebhookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_webhook_list(args: ListArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;
    let workspace_id = workspace_picker(&client, args.workspace_id).await?;

    let webhooks = webhooks_list(&client, workspace_id, args.page, args.limit).await?;

    match args.output {
        WebhookOutput::Table => {
            let rows: Vec<WebhookRow> = webhooks.into_iter().map(Into::into).collect();
            output_list(rows)
        }
        WebhookOutput::Json => output_json(&webhooks),
    }
}

#[derive(Parser)]
struct DeleteArgs {
    /// Which webhook should be deleted
    #[clap(long)]
    webhook_id: Option<Base64Uuid>,

    /// Workspace for which the webhook should be deleted for
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_webhook_delete(args: DeleteArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let webhook_id = webhook_picker(&client, workspace_id, args.webhook_id).await?;

    if !confirm(format!(
        "Are you sure you want to delete webhook {webhook_id}?"
    ))? {
        info!("Operation cancelled");
        return Ok(());
    }

    webhook_delete(&client, workspace_id, webhook_id).await?;

    info!("Successfully deleted webhook");
    Ok(())
}

#[derive(Parser)]
struct UpdateArgs {
    /// Which webhook should be updated
    #[clap(long)]
    webhook_id: Option<Base64Uuid>,

    /// New endpoint url for the webhook.
    #[clap(long)]
    endpoint: Option<String>,

    /// New categories for which the webhook should receive deliveries.
    /// Setting this option will override the already set categories with the passed ones.
    #[clap(long, value_enum)]
    categories: Option<Vec<WebhookCategory>>,

    /// Whenever the shared secret should be regenerated for this webhook
    #[clap(long, default_value = "false")]
    regenerate_shared_secret: bool,

    /// Whenever the webhook should be enabled and thus receive deliveries
    #[clap(long)]
    enabled: Option<bool>,

    /// Workspace for which the webhook should be updated for
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the webhook
    #[clap(long, short, default_value = "table", value_enum)]
    output: WebhookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_webhook_update(args: UpdateArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let webhook_id = webhook_picker(&client, workspace_id, args.webhook_id).await?;

    let mut payload = UpdateWebhook::builder()
        .regenerate_shared_secret(args.regenerate_shared_secret)
        .build();

    payload.endpoint = args.endpoint.clone();
    payload.events = args.categories;
    payload.enabled = args.enabled;

    let webhook = webhook_update(&client, workspace_id, webhook_id, payload).await?;

    if args.endpoint.is_some() && !webhook.successful {
        warn!("The webhook has been updated into the disabled state because it failed to handle the \"ping\" event.");
        warn!(
            "See documentation at https://docs.fiberplane.com/docs/webhooks for more information."
        );
    } else {
        info!("Successfully updated webhook.");
    }

    if args.regenerate_shared_secret {
        info!(
            "Don't forget to copy the shared secret, as this will be the last time you'll see it."
        );
    }

    match args.output {
        WebhookOutput::Table => output_details(GenericKeyValue::from_webhook(webhook)),
        WebhookOutput::Json => output_json(&webhook),
    }
}

#[derive(Parser)]
struct WebhookDeliveryListArgs {
    /// For which webhook to display deliveries
    #[clap(long)]
    webhook_id: Option<Base64Uuid>,

    /// Page to display
    #[clap(long)]
    page: Option<i32>,

    /// Amount of webhooks to display per page
    #[clap(long)]
    limit: Option<i32>,

    /// Workspace for which deliveries should be listed
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the webhooks
    #[clap(long, short, default_value = "table", value_enum)]
    output: WebhookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_webhook_delivery_list(args: WebhookDeliveryListArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let webhook_id = webhook_picker(&client, workspace_id, args.webhook_id).await?;

    let deliveries =
        webhook_delivery_list(&client, workspace_id, webhook_id, args.page, args.limit).await?;

    match args.output {
        WebhookOutput::Table => {
            let rows: Vec<WebhookDeliverySummaryRow> =
                deliveries.into_iter().map(Into::into).collect();
            output_list(rows)
        }
        WebhookOutput::Json => output_json(&deliveries),
    }
}

#[derive(Parser)]
struct WebhookDeliveryInfoArgs {
    /// For which webhook to display delivery info
    #[clap(long)]
    webhook_id: Option<Base64Uuid>,

    /// For which delivery to display info
    #[clap(long)]
    delivery_id: Option<Base64Uuid>,

    /// Workspace for which delivery info should be displayed
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the delivery
    #[clap(long, short, default_value = "table", value_enum)]
    output: WebhookDeliveryOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_webhook_delivery_info(args: WebhookDeliveryInfoArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let webhook_id = webhook_picker(&client, workspace_id, args.webhook_id).await?;
    let delivery_id =
        webhook_delivery_picker(&client, workspace_id, webhook_id, args.delivery_id).await?;

    let delivery = webhook_delivery_get(&client, workspace_id, webhook_id, delivery_id).await?;

    match args.output {
        WebhookDeliveryOutput::Table => {
            output_details(GenericKeyValue::from_webhook_delivery(delivery))?
        }
        WebhookDeliveryOutput::Json => output_json(&delivery)?,
        WebhookDeliveryOutput::RequestHeaders => println!("{}", delivery.request_headers),
        WebhookDeliveryOutput::RequestBody => println!("{}", delivery.request_body),
        WebhookDeliveryOutput::ResponseHeaders => println!(
            "{}",
            delivery
                .response_headers
                .unwrap_or_else(|| "No response headers received".to_string())
        ),
        WebhookDeliveryOutput::ResponseBody => println!(
            "{}",
            delivery
                .response_body
                .unwrap_or_else(|| "No response body received".to_string())
        ),
    }

    Ok(())
}

#[derive(Parser)]
struct WebhookDeliveryResendArgs {
    /// For which webhook to trigger a resend
    #[clap(long)]
    webhook_id: Option<Base64Uuid>,

    /// For which delivery to trigger a resend
    #[clap(long)]
    delivery_id: Option<Base64Uuid>,

    /// Workspace for which a delivery should be resent
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_webhook_delivery_resend(args: WebhookDeliveryResendArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let webhook_id = webhook_picker(&client, workspace_id, args.webhook_id).await?;
    let delivery_id =
        webhook_delivery_picker(&client, workspace_id, webhook_id, args.delivery_id).await?;

    webhook_delivery_resend(&client, workspace_id, webhook_id, delivery_id).await?;

    info!("Successfully triggered a resend on the delivery");
    Ok(())
}

#[derive(ValueEnum, Clone)]
enum WebhookOutput {
    /// Output the details of the webhook as a table
    Table,

    /// Output the webhook as JSON
    Json,
}

#[derive(ValueEnum, Clone, Debug)]
enum WebhookListOutput {
    /// Output the result as a table
    Table,

    /// Output the result as a JSON list
    Json,
}

#[derive(ValueEnum, Clone)]
enum WebhookDeliveryOutput {
    /// Output the details of the delivery as a table
    Table,

    /// Output the delivery as JSON
    Json,

    /// Output only the request headers
    RequestHeaders,

    /// Output only the request body
    RequestBody,

    /// Output only the response headers
    ResponseHeaders,

    /// Output only the response body
    ResponseBody,
}

impl GenericKeyValue {
    fn from_webhook(webhook: Webhook) -> Vec<Self> {
        let mut vec = vec![
            GenericKeyValue::new("ID:", webhook.id),
            GenericKeyValue::new("Workspace ID:", webhook.workspace_id),
            GenericKeyValue::new("Endpoint:", webhook.endpoint),
            GenericKeyValue::new("Categories:", print_categories(&webhook.events)),
            GenericKeyValue::new("Enabled:", webhook.enabled.to_string()),
            GenericKeyValue::new(
                "Created by:",
                webhook
                    .created_by
                    .map_or_else(|| "Unknown".to_string(), |id| id.to_string()),
            ),
            GenericKeyValue::new(
                "Created at:",
                webhook.created_at.format(&Rfc3339).unwrap_or_default(),
            ),
            GenericKeyValue::new(
                "Updated at:",
                webhook.updated_at.format(&Rfc3339).unwrap_or_default(),
            ),
        ];

        if let Some(shared_secret) = webhook.shared_secret {
            vec.insert(4, GenericKeyValue::new("Shared Secret:", shared_secret));
        }

        vec
    }

    fn from_webhook_delivery(delivery: WebhookDelivery) -> Vec<Self> {
        let mut vec = vec![
            GenericKeyValue::new("ID:", delivery.id),
            GenericKeyValue::new("Webhook ID:", delivery.webhook_id),
            GenericKeyValue::new("Event:", delivery.event),
            GenericKeyValue::new(
                "Delivered at:",
                delivery
                    .sent_request_at
                    .format(&Rfc3339)
                    .unwrap_or_default(),
            ),
        ];

        if let Some(status_code) = delivery.status_code {
            vec.push(GenericKeyValue::new(
                "Status Code:",
                status_code.to_string(),
            ));
        }

        if let Some(status_text) = delivery.status_text {
            vec.push(GenericKeyValue::new("Status Text:", status_text));
        }

        vec
    }
}

#[derive(Table)]
struct WebhookRow {
    #[table(title = "ID")]
    id: String,

    #[table(title = "Workspace ID")]
    workspace_id: String,

    #[table(title = "Endpoint")]
    endpoint: String,

    #[table(title = "Categories", display_fn = "print_categories")]
    events: Vec<WebhookCategory>,

    #[table(title = "Shared Secret")]
    shared_secret: String,

    #[table(title = "Endpoint")]
    enabled: String,

    #[table(title = "Last Delivery")]
    successful: String,

    #[table(title = "Created by")]
    created_by: String,

    #[table(title = "Created at")]
    created_at: String,

    #[table(title = "Updated at")]
    updated_at: String,
}

impl From<Webhook> for WebhookRow {
    fn from(webhook: Webhook) -> Self {
        Self {
            id: webhook.id.to_string(),
            workspace_id: webhook.workspace_id.to_string(),
            endpoint: webhook.endpoint,
            events: webhook.events,
            shared_secret: webhook
                .shared_secret
                .unwrap_or_else(|| "[hidden]".to_string()),
            enabled: webhook.enabled.to_string(),
            successful: if webhook.successful {
                "Successful".to_string()
            } else {
                "Failed".to_string()
            },
            created_by: webhook
                .created_by
                .map_or_else(|| "Unknown".to_string(), |id| id.to_string()),
            created_at: webhook.created_at.format(&Rfc3339).unwrap_or_default(),
            updated_at: webhook.updated_at.format(&Rfc3339).unwrap_or_default(),
        }
    }
}

#[derive(Table)]
struct WebhookDeliverySummaryRow {
    #[table(title = "ID")]
    id: String,

    #[table(title = "Event")]
    event: String,

    #[table(title = "Successful")]
    successful: String,

    #[table(title = "Delivered at")]
    timestamp: String,
}

impl From<WebhookDeliverySummary> for WebhookDeliverySummaryRow {
    fn from(delivery: WebhookDeliverySummary) -> Self {
        Self {
            id: delivery.id.to_string(),
            event: delivery.event,
            successful: delivery.successful.to_string(),
            timestamp: delivery.timestamp.format(&Rfc3339).unwrap_or_default(),
        }
    }
}

fn print_categories(input: &[WebhookCategory]) -> String {
    input
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
