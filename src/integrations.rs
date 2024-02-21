use crate::config::api_client_configuration;
use crate::output::{output_json, output_list};
use anyhow::Result;
use clap::{Parser, ValueEnum};
use cli_table::Table;
use fiberplane::api_client::integrations_get;
use fiberplane::models::integrations::IntegrationSummary;
use std::path::PathBuf;
use time::format_description::well_known::Rfc3339;
use url::Url;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    /// List all integrations
    List(ListArgs),
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::List(args) => handle_integrations_list(args).await,
    }
}

#[derive(Parser)]
struct ListArgs {
    /// Output of the webhooks
    #[clap(long, short, default_value = "table", value_enum)]
    output: IntegrationOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    #[clap(from_global)]
    token: Option<String>,
}

#[derive(ValueEnum, Clone)]
enum IntegrationOutput {
    /// Output the details of the integrations as a table
    Table,

    /// Output the integration as JSON
    Json,
}

async fn handle_integrations_list(args: ListArgs) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url).await?;
    let integrations = integrations_get(&client).await?;

    match args.output {
        IntegrationOutput::Table => {
            let rows: Vec<IntegrationRow> = integrations.into_iter().map(Into::into).collect();
            output_list(rows)
        }
        IntegrationOutput::Json => output_json(&integrations),
    }
}

#[derive(Table)]
struct IntegrationRow {
    #[table(title = "ID")]
    id: String,

    #[table(title = "Status")]
    status: String,

    #[table(title = "Created at")]
    created_at: String,

    #[table(title = "Updated at")]
    updated_at: String,
}

impl From<IntegrationSummary> for IntegrationRow {
    fn from(integration: IntegrationSummary) -> Self {
        Self {
            id: integration.id.to_string(),
            status: integration.status.to_string(),
            created_at: integration.created_at.map_or_else(
                || "n/a".to_string(),
                |time| time.format(&Rfc3339).unwrap_or_default(),
            ),
            updated_at: integration.updated_at.map_or_else(
                || "n/a".to_string(),
                |time| time.format(&Rfc3339).unwrap_or_default(),
            ),
        }
    }
}
