use crate::config::api_client_configuration;
use anyhow::{anyhow, Error, Result};
use clap::Parser;
use fp_api_client::apis::default_api::{get_notebook, notebook_create};
use fp_api_client::models::{Label, NewNotebook, TimeRange};
use std::io::{self, BufWriter};
use std::str::FromStr;
use std::time::Duration;
use time::OffsetDateTime;
use time_util::clap_rfc3339;
use tracing::{info, trace};

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
pub enum SubCommand {
    #[clap(name = "add", aliases = &["a"], about = "Creates an empty notebook")]
    Add(AddArgs),

    #[clap(name = "get", aliases = &["g"], about = "Get an notebook as JSON")]
    Get(GetArgs),
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Add(args) => handle_add_command(args).await,
        Get(args) => handle_get_command(args).await,
    }
}

#[derive(Parser)]
pub struct AddArgs {
    /// Title for the new notebook
    #[clap(short, long)]
    title: Option<String>,

    /// Labels to attach to the newly created notebook (you can specify multiple labels).
    #[clap(name = "label", short, long)]
    labels: Vec<KeyValue>,

    /// Start time to be passed into the new notebook. Leave empty to use 60 minutes ago.
    #[clap(long, parse(try_from_str = clap_rfc3339::parse_rfc3339))]
    from: Option<OffsetDateTime>,

    /// End time to be passed into the new notebook. Leave empty to use the current time.
    #[clap(long, parse(try_from_str = clap_rfc3339::parse_rfc3339))]
    to: Option<OffsetDateTime>,

    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,
}

async fn handle_add_command(args: AddArgs) -> Result<()> {
    let title = args.title.unwrap_or_else(|| String::from("new title"));

    let labels = match args.labels.len() {
        0 => None,
        _ => Some(
            args.labels
                .into_iter()
                .map(|input| Label {
                    key: input.key,
                    value: input.value,
                })
                .collect(),
        ),
    };

    // Currently both `from` and `to` only parse up to second precession.
    let from = args
        .from
        .unwrap_or_else(|| OffsetDateTime::now_utc() - Duration::from_secs(60 * 60))
        .unix_timestamp() as f32;

    let to = args
        .from
        .unwrap_or_else(OffsetDateTime::now_utc)
        .unix_timestamp() as f32;

    let notebook = NewNotebook {
        title,
        time_range: Box::new(TimeRange { from, to }),
        cells: vec![],
        data_sources: None,
        labels,
    };

    let config = api_client_configuration(args.config.as_deref(), &args.base_url).await?;
    trace!(?notebook, "creating new notebook");
    let notebook = notebook_create(&config, Some(notebook)).await?;

    info!("Successfully created new notebook");
    println!("{}", notebook_url(args.base_url, notebook.id));

    Ok(())
}

#[derive(Parser)]
pub struct GetArgs {
    // ID of the notebook
    #[clap(name = "id")]
    id: String,

    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,
}

async fn handle_get_command(args: GetArgs) -> Result<()> {
    let config = api_client_configuration(args.config.as_deref(), &args.base_url).await?;
    trace!(id = ?args.id, "fetching notebook");

    let notebook = get_notebook(&config, &args.id).await?;

    let writer = BufWriter::new(io::stdout());
    serde_json::to_writer_pretty(writer, &notebook)?;

    Ok(())
}

fn notebook_url(base_url: String, id: String) -> String {
    format!("{}/notebook/{}", base_url, id)
}

pub struct KeyValue {
    pub key: String,
    pub value: String,
}

impl FromStr for KeyValue {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        if s.is_empty() {
            return Err(anyhow!("empty input"));
        }

        let (key, value) = match s.split_once('=') {
            Some((key, value)) => (key, value),
            None => (s, ""),
        };

        Ok(KeyValue {
            key: key.to_owned(),
            value: value.to_owned(),
        })
    }
}
