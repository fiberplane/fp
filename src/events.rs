use crate::config::api_client_configuration;
use crate::interactive::workspace_picker;
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use crate::{interactive, KeyValueArgument};
use anyhow::Result;
use base64uuid::Base64Uuid;
use clap::{Parser, ValueEnum};
use cli_table::Table;
use fiberplane::sorting::{EventSortFields, SortDirection};
use fp_api_client::apis::default_api::{event_create, event_delete, event_list};
use fp_api_client::models::{Event, NewEvent};
use std::collections::HashMap;
use std::fmt::Display;
use std::path::PathBuf;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use time_util::clap_rfc3339;
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
        Create(args) => handle_event_create_command(args).await,
        Search(args) => handle_event_search_command(args).await,
        Delete(args) => handle_event_delete_command(args).await,
    }
}

#[derive(Parser)]
enum SubCommand {
    /// Create an event
    #[clap(alias = "add")]
    Create(CreateArguments),

    /// Search for an event
    Search(SearchArguments),

    /// Delete an event
    #[clap(aliases = &["remove", "rm"])]
    Delete(DeleteArguments),
}

#[derive(ValueEnum, Clone)]
enum EventOutput {
    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,
}

#[derive(Parser)]
struct CreateArguments {
    /// Name of the event
    #[clap(long, alias = "name")]
    title: Option<String>,

    /// Labels to add to the events (you can specify multiple labels).
    #[clap(name = "label", short, long)]
    labels: Vec<KeyValueArgument>,

    /// Time at which the event occurred. Leave empty to use current time.
    #[clap(long, value_parser = clap_rfc3339::parse_rfc3339)]
    time: Option<OffsetDateTime>,

    /// Output of the event
    #[clap(long, short, default_value = "table", value_enum)]
    output: EventOutput,

    /// Workspace to create the event in.
    #[clap(long, short, env)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct SearchArguments {
    /// Labels to search events for (you can specify multiple labels).
    #[clap(name = "label", short, long, required = true)]
    labels: Vec<KeyValueArgument>,

    /// Start time to search for events for
    #[clap(long, value_parser = clap_rfc3339::parse_rfc3339, required = true)]
    start: OffsetDateTime,

    /// End time to search for events for
    #[clap(long, value_parser = clap_rfc3339::parse_rfc3339, required = true)]
    end: OffsetDateTime,

    /// Output of the event
    #[clap(long, short, default_value = "table", value_enum)]
    output: EventOutput,

    /// Workspace to search for events in.
    #[clap(long, short, env)]
    workspace_id: Option<Base64Uuid>,

    /// Sort the result according to the following field
    #[clap(long, value_enum)]
    sort_by: Option<EventSortFields>,

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

async fn handle_event_create_command(args: CreateArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let key_values: HashMap<String, String> = args
        .labels
        .into_iter()
        .map(|kv| (kv.key, kv.value))
        .collect();
    let labels = if !key_values.is_empty() {
        Some(key_values)
    } else {
        None
    };

    let title = interactive::text_req("Title", args.title, None)?;
    let time = args.time.map(|input| input.format(&Rfc3339).unwrap());
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    let event = event_create(
        &config,
        &workspace_id.to_string(),
        NewEvent {
            title,
            labels,
            time,
        },
    )
    .await?;

    info!("Successfully created new event");

    match args.output {
        EventOutput::Table => output_details(GenericKeyValue::from_event(event)),
        EventOutput::Json => output_json(&event),
    }
}

async fn handle_event_search_command(args: SearchArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    let events = event_list(
        &config,
        &workspace_id.to_string(),
        args.start.format(&Rfc3339)?,
        args.end.format(&Rfc3339)?,
        Some(
            args.labels
                .into_iter()
                .map(|kv| (kv.key, kv.value))
                .collect(),
        ),
        args.sort_by.map(Into::into),
        args.sort_direction.map(Into::into),
        args.page,
        args.limit,
    )
    .await?;

    match args.output {
        EventOutput::Table => {
            let rows: Vec<EventRow> = events.into_iter().map(Into::into).collect();
            output_list(rows)
        }
        EventOutput::Json => output_json(&events),
    }
}

#[derive(Parser)]
pub struct DeleteArguments {
    /// ID of the event that should be deleted
    id: Base64Uuid,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_event_delete_command(args: DeleteArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    event_delete(&config, &args.id.to_string()).await?;

    info!("Successfully deleted event");
    Ok(())
}

#[derive(Table)]
struct EventRow {
    #[table(title = "ID")]
    id: String,

    #[table(title = "Title")]
    title: String,

    #[table(title = "Labels", display_fn = "print_labels")]
    labels: HashMap<String, String>,

    #[table(title = "Time")]
    time: String,
}

impl From<Event> for EventRow {
    fn from(event: Event) -> Self {
        EventRow {
            id: event.id,
            title: event.title,
            labels: event.labels,
            time: event.occurrence_time,
        }
    }
}

fn print_labels(input: &HashMap<String, String>) -> impl Display {
    let mut output = String::new();
    let mut iterator = input.iter().peekable();

    while let Some((key, value)) = iterator.next() {
        output.push_str(key);

        if !value.is_empty() {
            output.push('=');
            output.push_str(value);
        }

        if iterator.peek().is_some() {
            output.push_str(", ");
        }
    }

    output
}

impl GenericKeyValue {
    fn from_event(event: Event) -> Vec<Self> {
        vec![
            GenericKeyValue::new("Title:", event.title),
            GenericKeyValue::new("Labels:", format!("{}", print_labels(&event.labels))),
            GenericKeyValue::new("Occurrence Time:", event.occurrence_time),
            GenericKeyValue::new("ID:", event.id),
        ]
    }
}
