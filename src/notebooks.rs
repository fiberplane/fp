use crate::config::api_client_configuration;
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use crate::KeyValueArgument;
use anyhow::{anyhow, Context, Result};
use clap::{ArgEnum, Parser, ValueHint};
use cli_table::Table;
use fiberplane::protocols::core;
use fiberplane_markdown::{markdown_to_notebook, notebook_to_markdown};
use fp_api_client::apis::default_api::{
    delete_notebook, get_notebook, notebook_cells_append, notebook_create, notebook_list,
    notebook_search,
};
use fp_api_client::models::{
    Cell, Label, NewNotebook, Notebook, NotebookSearch, NotebookSummary, NotebookVisibility,
    TimeRange,
};
use std::{collections::HashMap, path::PathBuf, time::Duration};
use time::OffsetDateTime;
use time_util::clap_rfc3339;
use tracing::{info, trace};
use url::Url;
use webbrowser::open;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
pub enum SubCommand {
    /// Create a notebook
    Create(CreateArgs),

    /// Retrieve a notebook
    Get(GetArgs),

    /// List all notebooks
    List(ListArgs),

    /// Search for a specific notebook
    /// This currently only supports label search
    Search(SearchArgs),

    /// Open a notebook in the studio
    Open(OpenArgs),

    /// Delete a notebook
    Delete(DeleteArgs),

    /// Append a cell to the notebook
    #[clap(alias = "append")]
    AppendCell(AppendCellArgs),
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Create(args) => handle_create_command(args).await,
        Get(args) => handle_get_command(args).await,
        List(args) => handle_list_command(args).await,
        Search(args) => handle_search_command(args).await,
        Open(args) => handle_open_command(args).await,
        Delete(args) => handle_delete_command(args).await,
        AppendCell(args) => handle_append_cell_command(args).await,
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

    /// Create the notebook from the given Markdown
    ///
    /// To read the Markdown from a file use `--markdown=$(cat file.md)`
    #[clap(long, short, value_hint = ValueHint::FilePath)]
    markdown: Option<String>,

    /// Output of the notebook
    #[clap(long, short, default_value = "table", arg_enum)]
    output: NotebookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct GetArgs {
    /// ID of the notebook
    id: String,

    /// Output of the notebook
    #[clap(long, short, default_value = "table", arg_enum)]
    output: SingleNotebookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct ListArgs {
    /// Output of the notebook
    #[clap(long, short, default_value = "table", arg_enum)]
    output: NotebookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct SearchArgs {
    /// Labels to search notebooks for (you can specify multiple labels).
    #[clap(name = "label", short, long)]
    labels: Vec<KeyValueArgument>,

    /// Output of the notebooks
    #[clap(long, short, default_value = "table", arg_enum)]
    output: NotebookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct OpenArgs {
    /// ID of the notebook
    id: String,

    #[clap(from_global)]
    base_url: Url,
}

#[derive(Parser)]
pub struct DeleteArgs {
    /// ID of the notebook
    id: String,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct AppendCellArgs {
    /// ID of the notebook
    id: String,

    /// Append a text cell
    #[clap(long, conflicts_with_all = &["code"])]
    text: Option<String>,

    /// Append a code cell
    #[clap(long, conflicts_with_all = &["text"])]
    code: Option<String>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    /// Output type to display
    #[clap(long, short, default_value = "table", arg_enum)]
    output: CellOutput,
}

/// A generic output for notebook related commands.
#[derive(ArgEnum, Clone)]
enum SingleNotebookOutput {
    /// Output the result as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,

    /// Output the notebook as Markdown
    Markdown,
}

/// A generic output for notebook related commands.
#[derive(ArgEnum, Clone)]
enum NotebookOutput {
    /// Output the result as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

/// Output for cell related commands
#[derive(ArgEnum, Clone)]
enum CellOutput {
    /// Output the result as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

async fn handle_create_command(args: CreateArgs) -> Result<()> {
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

    // Optionally parse the notebook from Markdown
    let notebook = match args.markdown {
        Some(markdown) => {
            let notebook = markdown_to_notebook(&markdown);
            let notebook = serde_json::to_string(&notebook)?;
            serde_json::from_str(&notebook).with_context(|| "Error parsing notebook struct (there is a mismatch between the API client model and the fiberplane core model)")?
        }
        None => NewNotebook {
            title: String::new(),
            time_range: Box::new(TimeRange { from, to }),
            cells: Vec::new(),
            data_sources: None,
            labels: Default::default(),
        },
    };
    let title = if let Some(title) = args.title {
        title
    } else if !notebook.title.is_empty() {
        notebook.title
    } else {
        "New Notebook".to_string()
    };
    let notebook = NewNotebook {
        title,
        time_range: Box::new(TimeRange { from, to }),
        labels,
        ..notebook
    };

    let config = api_client_configuration(args.config, &args.base_url).await?;
    let notebook = notebook_create(&config, notebook).await?;

    match args.output {
        NotebookOutput::Table => {
            info!("Successfully created new notebook");
            println!("{}", notebook_url(args.base_url, notebook.id));
            Ok(())
        }
        NotebookOutput::Json => output_json(&notebook),
    }
}

async fn handle_get_command(args: GetArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    trace!(id = ?args.id, "fetching notebook");

    let notebook = get_notebook(&config, &args.id).await?;

    match args.output {
        SingleNotebookOutput::Table => output_details(GenericKeyValue::from_notebook(notebook)),
        SingleNotebookOutput::Json => output_json(&notebook),
        SingleNotebookOutput::Markdown => {
            let notebook = serde_json::to_string(&notebook)?;
            let notebook: core::Notebook = serde_json::from_str(&notebook)?;
            let markdown = notebook_to_markdown(notebook);
            println!("{}", markdown);
            Ok(())
        }
    }
}

async fn handle_list_command(args: ListArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let notebooks = notebook_list(&config).await?;

    match args.output {
        NotebookOutput::Table => {
            let mut notebooks: Vec<NotebookSummaryRow> =
                notebooks.into_iter().map(Into::into).collect();

            // Sort by updated at so that the most recent is first
            notebooks.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

            output_list(notebooks)
        }
        NotebookOutput::Json => output_json(&notebooks),
    }
}

async fn handle_search_command(args: SearchArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let labels: Option<HashMap<String, String>> = if args.labels.len() != 0 {
        Some(
            args.labels
                .into_iter()
                .map(|kv| (kv.key, kv.value))
                .collect(),
        )
    } else {
        None
    };

    let notebooks = notebook_search(&config, NotebookSearch { labels }).await?;

    match args.output {
        NotebookOutput::Table => {
            let notebooks: Vec<NotebookSummaryRow> =
                notebooks.into_iter().map(Into::into).collect();

            output_list(notebooks)
        }
        NotebookOutput::Json => output_json(&notebooks),
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

async fn handle_append_cell_command(args: AppendCellArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let cell = if let Some(content) = args.text {
        Cell::TextCell {
            content,
            id: String::new(),
            formatting: None,
            read_only: None,
        }
    } else if let Some(content) = args.code {
        Cell::CodeCell {
            content,
            id: String::new(),
            syntax: None,
            read_only: None,
        }
    } else {
        panic!("Must provide a cell type");
    };

    let cell = notebook_cells_append(&config, &args.id, vec![cell])
        .await?
        .pop()
        .ok_or_else(|| anyhow!("Expected a single cell"))?;
    match args.output {
        CellOutput::Json => output_json(&cell),
        CellOutput::Table => output_details(GenericKeyValue::from_cell(cell)),
    }
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

    pub fn from_cell(cell: Cell) -> Vec<GenericKeyValue> {
        let (id, cell_type) = match cell {
            Cell::TextCell { id, .. } => (id, "Text"),
            Cell::CodeCell { id, .. } => (id, "Code"),
            _ => unimplemented!(),
        };
        vec![
            GenericKeyValue::new("Cell ID:", id),
            GenericKeyValue::new("Cell Type:", cell_type),
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
