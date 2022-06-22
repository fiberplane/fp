use crate::config::api_client_configuration;
use crate::output::output_details;
use crate::KeyValueArgument;
use anyhow::Result;
use clap::Parser;
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
    Create(CreateArguments),

    /// Search for an event
    Search(SearchArguments),

    /// Delete an event
    Delete(DeleteArguments),
}

#[derive(Parser)]
struct CreateArguments {
    /// Name of the event
    #[clap(long, alias = "name")]
    title: String,

    /// Labels to add to the events (you can specify multiple labels).
    #[clap(name = "label", short, long)]
    labels: Vec<KeyValueArgument>,

    /// Time at which the event occurred. Leave empty to use current time.
    #[clap(long, parse(try_from_str = clap_rfc3339::parse_rfc3339))]
    time: Option<OffsetDateTime>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct SearchArguments {
    /// Labels to search events for (you can specify multiple labels).
    #[clap(name = "label", short, long)]
    labels: Vec<KeyValueArgument>,

    /// Start time to search for events for
    #[clap(long, parse(try_from_str = clap_rfc3339::parse_rfc3339))]
    start: OffsetDateTime,

    /// End time to search for events for
    #[clap(long, parse(try_from_str = clap_rfc3339::parse_rfc3339))]
    end: OffsetDateTime,

    /// Sort the result according to the following field
    #[clap(long, arg_enum)]
    sort_by: Option<EventSortFields>,

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
pub struct DeleteArguments {
    /// ID of the event that should be deleted
    #[clap(long, short)]
    id: String,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_event_create_command(args: CreateArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let formatted = if let Some(time) = args.time {
        Some(time.format(&Rfc3339)?)
    } else {
        None
    };

    event_create(
        &config,
        NewEvent {
            title: args.title,
            labels: Some(
                args.labels
                    .into_iter()
                    .map(|kv| (kv.key, kv.value))
                    .collect(),
            ),
            time: formatted,
        },
    )
    .await?;

    info!("Successfully created new event");
    Ok(())
}

async fn handle_event_search_command(args: SearchArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let events = event_list(
        &config,
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

    let rows: Vec<EventRow> = events.into_iter().map(Into::into).collect();
    output_details(rows)
}

async fn handle_event_delete_command(args: DeleteArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    event_delete(&config, &args.id).await?;

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
            id: event.id.unwrap_or_default(),
            title: event.title.unwrap_or_default(),
            labels: event.labels.unwrap_or_default(),
            time: event.occurrence_time.unwrap_or_default(),
        }
    }
}

fn print_labels(input: &HashMap<String, String>) -> impl Display {
    let mut output = String::new();

    for (key, value) in input {
        output.push_str(key);

        if !value.is_empty() {
            output.push('=');
            output.push_str(value);
        }

        output.push_str(", ");
    }

    output
}
