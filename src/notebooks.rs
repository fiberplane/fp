use crate::config::api_client_configuration;
use crate::output::{output_details, output_list, GenericKeyValue};
use crate::KeyValueArgument;
use anyhow::{Context, Result};
use clap::{ArgEnum, Parser};
use cli_table::Table;
use fp_api_client::apis::default_api::{
    delete_notebook, get_notebook, notebook_create, notebook_list,
};
use fp_api_client::models::{
    Label, NewNotebook, Notebook, NotebookSummary, NotebookVisibility, TimeRange,
};
use std::io::Write;
use std::io::{self, BufWriter};
use std::path::PathBuf;
use std::time::Duration;
use time::OffsetDateTime;
use time_util::clap_rfc3339;
use tracing::{debug, info, trace};
use url::Url;
use webbrowser::open;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
pub enum SubCommand {
    /// Create a new notebook
    #[clap()]
    Create(CreateArgs),

    /// Retrieve a single notebook
    #[clap()]
    Get(GetArgs),

    /// List all notebooks
    #[clap()]
    List(ListArgs),

    /// Open a notebook in the studio
    Open(OpenArgs),

    /// Delete a single notebook
    #[clap()]
    Delete(DeleteArgs),
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Create(args) => handle_add_command(args).await,
        Get(args) => handle_get_command(args).await,
        List(args) => handle_list_command(args).await,
        Open(args) => handle_open_command(args).await,
        Delete(args) => handle_delete_command(args).await,
    }
}

#[derive(Parser)]
pub struct CreateArgs {
    /// Title for the new notebook
    #[clap(short, long)]
    title: Option<String>,

    /// Labels to attach to the newly created notebook (you can specify multiple labels).
    #[clap(name = "label", short, long)]
    labels: Vec<KeyValueArgument>,

    /// Start time to be passed into the new notebook (RFC3339). Leave empty to use 60 minutes ago.
    #[clap(long, parse(try_from_str = clap_rfc3339::parse_rfc3339))]
    from: Option<OffsetDateTime>,

    /// End time to be passed into the new notebook (RFC3339). Leave empty to use the current time.
    #[clap(long, parse(try_from_str = clap_rfc3339::parse_rfc3339))]
    to: Option<OffsetDateTime>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_add_command(args: CreateArgs) -> Result<()> {
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

    let config = api_client_configuration(args.config, &args.base_url).await?;

    debug!(?notebook, "creating new notebook");
    let notebook = notebook_create(&config, Some(notebook)).await?;

    info!("Successfully created new notebook");
    println!("{}", notebook_url(args.base_url, notebook.id));

    Ok(())
}

#[derive(Parser)]
pub struct GetArgs {
    /// ID of the notebook
    #[clap()]
    id: String,

    /// Output of the notebook
    #[clap(long, short, default_value = "table", arg_enum)]
    output: NotebookGetOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(ArgEnum, Clone)]
enum NotebookGetOutput {
    /// Output the details of the notebook as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

#[derive(Parser)]
pub struct ListArgs {
    /// Output of the notebook
    #[clap(long, short, default_value = "table", arg_enum)]
    output: NotebookListOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(ArgEnum, Clone)]
enum NotebookListOutput {
    /// Output the details of the notebook as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

#[derive(Parser)]
pub struct OpenArgs {
    /// ID of the notebook
    #[clap()]
    id: String,

    #[clap(from_global)]
    base_url: Url,
}

#[derive(Parser)]
pub struct DeleteArgs {
    /// ID of the notebook
    #[clap()]
    id: String,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_get_command(args: GetArgs) -> Result<()> {
    use NotebookGetOutput::*;

    let config = api_client_configuration(args.config, &args.base_url).await?;
    trace!(id = ?args.id, "fetching notebook");

    let notebook = get_notebook(&config, &args.id).await?;

    match args.output {
        Table => output_details(GenericKeyValue::from_notebook(notebook)),
        Json => {
            let mut writer = BufWriter::new(io::stdout());
            serde_json::to_writer_pretty(&mut writer, &notebook)?;
            writeln!(writer)?;
            Ok(())
        }
    }
}

async fn handle_list_command(args: ListArgs) -> Result<()> {
    use NotebookListOutput::*;

    let config = api_client_configuration(args.config, &args.base_url).await?;
    let notebooks = notebook_list(&config).await?;

    match args.output {
        Table => {
            let mut notebooks: Vec<NotebookSummaryRow> =
                notebooks.into_iter().map(Into::into).collect();

            // Sort by updated at so that the most recent is first
            notebooks.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

            output_list(notebooks)
        }
        Json => {
            let mut writer = BufWriter::new(io::stdout());
            serde_json::to_writer_pretty(&mut writer, &notebooks)?;
            writeln!(writer)?;
            Ok(())
        }
    }
}

async fn handle_open_command(args: OpenArgs) -> Result<()> {
    let url = notebook_url(args.base_url, args.id);
    if open(&url).is_err() {
        info!("Please go to {} to view the notebook", url);
    }

    Ok(())
}

async fn handle_delete_command(args: DeleteArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let notebook_id = args.id;

    delete_notebook(&config, &notebook_id.to_string())
        .await
        .with_context(|| format!("Error deleting notebook {}", notebook_id))?;

    info!(%notebook_id, "Deleted notebook");
    Ok(())
}

fn notebook_url(base_url: Url, id: String) -> String {
    format!("{}notebook/{}", base_url, id)
}

impl GenericKeyValue {
    pub fn from_notebook(notebook: Notebook) -> Vec<GenericKeyValue> {
        let visibility = notebook
            .visibility
            .unwrap_or(NotebookVisibility::Private)
            .to_string();

        let labels = if notebook.labels.is_empty() {
            String::from("(none)")
        } else {
            let labels: Vec<_> = notebook
                .labels
                .into_iter()
                .map(|label| {
                    if label.value.is_empty() {
                        label.key
                    } else {
                        format!("{}={}", label.key, label.value)
                    }
                })
                .collect();
            labels.join("\n")
        };

        vec![
            GenericKeyValue::new("Title:", notebook.title),
            GenericKeyValue::new("ID:", notebook.id),
            GenericKeyValue::new("Created by:", notebook.created_by.name),
            GenericKeyValue::new("Visibility:", visibility),
            GenericKeyValue::new("Updated at:", notebook.updated_at),
            GenericKeyValue::new("Created at:", notebook.created_at),
            GenericKeyValue::new("Current revision:", notebook.revision.to_string()),
            GenericKeyValue::new("Label:", labels),
        ]
    }
}

#[derive(Table)]
pub struct NotebookSummaryRow {
    #[table(title = "Title")]
    pub title: String,

    #[table(title = "ID")]
    pub id: String,

    #[table(title = "Created by")]
    pub created_by: String,

    #[table(title = "Visibility")]
    pub visibility: String,

    #[table(title = "Updated at")]
    pub updated_at: String,

    #[table(title = "Created at")]
    pub created_at: String,
}

impl From<NotebookSummary> for NotebookSummaryRow {
    fn from(notebook: NotebookSummary) -> Self {
        let visibility = notebook
            .visibility
            .unwrap_or(NotebookVisibility::Private)
            .to_string();
        Self {
            id: notebook.id,
            title: notebook.title,
            created_by: notebook.created_by.name,
            visibility,
            updated_at: notebook.updated_at,
            created_at: notebook.created_at,
        }
    }
}
