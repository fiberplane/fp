use crate::config::api_client_configuration;
use crate::interactive::{self, workspace_picker};
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use crate::KeyValueArgument;
use anyhow::Result;
use clap::{Parser, ValueEnum};
use cli_table::Table;
use fiberplane::api_client::{event_create, event_delete, event_list};
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::events::{Event, NewEvent};
use fiberplane::models::sorting::{EventSortFields, SortDirection};
use fiberplane::models::timestamps::Timestamp;
use std::{collections::HashMap, fmt::Display, path::PathBuf};
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
    #[clap(long)]
    time: Option<Timestamp>,

    /// Output of the event
    #[clap(long, short, default_value = "table", value_enum)]
    output: EventOutput,

    /// Workspace to create the event in.
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    #[clap(from_global)]
    token: Option<String>,
}

#[derive(Parser)]
pub struct SearchArguments {
    /// Labels to search events for (you can specify multiple labels).
    #[clap(name = "label", short, long)]
    labels: Option<Vec<KeyValueArgument>>,

    /// Start time to search for events for
    #[clap(long, required = true)]
    start: Timestamp,

    /// End time to search for events for
    #[clap(long, required = true)]
    end: Timestamp,

    /// Output of the event
    #[clap(long, short, default_value = "table", value_enum)]
    output: EventOutput,

    /// Workspace to search for events in.
    #[clap(from_global)]
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

    #[clap(from_global)]
    token: Option<String>,
}

async fn handle_event_create_command(args: CreateArguments) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url).await?;

    let key_values: HashMap<_, _> = args
        .labels
        .into_iter()
        .map(|kv| (kv.key, Some(kv.value)))
        .collect();
    let labels = if !key_values.is_empty() {
        Some(key_values)
    } else {
        None
    };

    let title = interactive::text_req("Title", args.title, None)?;
    let workspace_id = workspace_picker(&client, args.workspace_id).await?;

    let mut new_event = NewEvent::builder()
        .title(title)
        .labels(labels.unwrap_or_default())
        .build();
    new_event.time = args.time;
    let event = event_create(&client, workspace_id, new_event).await?;

    info!("Successfully created new event");

    match args.output {
        EventOutput::Table => output_details(GenericKeyValue::from_event(event)),
        EventOutput::Json => output_json(&event),
    }
}

async fn handle_event_search_command(args: SearchArguments) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;

    let events = event_list(
        &client,
        workspace_id,
        args.start,
        args.end,
        args.labels
            .map(|args| args.into_iter().map(|kv| (kv.key, kv.value)).collect()),
        args.sort_by.map(Into::<&str>::into),
        args.sort_direction.map(Into::<&str>::into),
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

    #[clap(from_global)]
    token: Option<String>,
}

async fn handle_event_delete_command(args: DeleteArguments) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url).await?;

    event_delete(&client, args.id).await?;

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
    labels: HashMap<String, Option<String>>,

    #[table(title = "Time")]
    time: String,
}

impl From<Event> for EventRow {
    fn from(event: Event) -> Self {
        EventRow {
            id: event.id.to_string(),
            title: event.title,
            labels: event.labels,
            time: event.occurrence_time.to_string(),
        }
    }
}

fn print_labels(input: &HashMap<String, Option<String>>) -> impl Display {
    let mut output = String::new();
    let mut iterator = input.iter().peekable();

    while let Some((key, value)) = iterator.next() {
        output.push_str(key);

        if let Some(value) = value {
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
            GenericKeyValue::new("Occurrence Time:", event.occurrence_time.to_string()),
            GenericKeyValue::new("ID:", event.id.to_string()),
        ]
    }
}
