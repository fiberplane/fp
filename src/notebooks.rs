use crate::interactive::{
    self, notebook_picker, snippet_picker, view_picker, workspace_picker,
    workspace_picker_with_prompt,
};
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use crate::KeyValueArgument;
use crate::{config::api_client_configuration, fp_urls::NotebookUrlBuilder};
use anyhow::{anyhow, Context, Result};
use clap::{Parser, ValueEnum, ValueHint};
use cli_table::Table;
use fiberplane::api_client::{
    front_matter_delete, front_matter_update, notebook_cells_append, notebook_create,
    notebook_delete, notebook_duplicate, notebook_get, notebook_list, notebook_search,
    notebook_snippet_insert,
};
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::markdown::{markdown_to_notebook, notebook_to_markdown};
use fiberplane::models::names::Name;
use fiberplane::models::notebooks;
use fiberplane::models::notebooks::{
    Cell, CodeCell, FrontMatter, NewNotebook, Notebook, NotebookCopyDestination, NotebookSearch,
    NotebookSummary, TextCell,
};
use fiberplane::models::sorting::{NotebookSortFields, SortDirection};
use fiberplane::models::timestamps::{NewTimeRange, TimeRange, Timestamp};
use std::path::PathBuf;
use time::ext::NumericalDuration;
use tracing::info;
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
    #[clap(alias = "add")]
    Create(CreateArgs),

    /// Duplicate a notebook
    #[clap(aliases = &["dup", "clone"])]
    Duplicate(DuplicateArgs),

    /// Retrieve a notebook
    Get(GetArgs),

    /// Insert a snippet into the notebook
    InsertSnippet(InsertSnippetArgs),

    /// List all notebooks
    List(ListArgs),

    /// Search for a specific notebook
    /// This currently only supports label search
    Search(SearchArgs),

    /// Open a notebook in the studio
    Open(OpenArgs),

    /// Delete a notebook
    #[clap(aliases = &["remove", "rm"])]
    Delete(DeleteArgs),

    /// Append a cell to the notebook
    #[clap(alias = "append")]
    AppendCell(AppendCellArgs),

    /// Interact with front matter
    ///
    /// Front matter adds additional metadata to notebooks.
    #[clap(alias = "fm")]
    FrontMatter(FrontMatterArguments),
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Create(args) => handle_create_command(args).await,
        Duplicate(args) => handle_duplicate_command(args).await,
        Get(args) => handle_get_command(args).await,
        InsertSnippet(args) => handle_insert_snippet_command(args).await,
        List(args) => handle_list_command(args).await,
        Search(args) => handle_search_command(args).await,
        Open(args) => handle_open_command(args).await,
        Delete(args) => handle_delete_command(args).await,
        AppendCell(args) => handle_append_cell_command(args).await,
        FrontMatter(args) => handle_front_matter_command(args).await,
    }
}

/// A generic output for notebook related commands.
#[derive(ValueEnum, Clone)]
enum SingleNotebookOutput {
    /// Output the result as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,

    /// Output the notebook as Markdown
    Markdown,
}

/// A generic output for notebook related commands.
#[derive(ValueEnum, Clone)]
enum NotebookOutput {
    /// Output the result as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

/// Output for cell related commands
#[derive(ValueEnum, Clone)]
enum CellOutput {
    /// Output the result as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

#[derive(Parser)]
pub struct CreateArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Title for the new notebook
    #[clap(short, long)]
    title: Option<String>,

    /// Labels to attach to the newly created notebook (you can specify multiple labels).
    #[clap(name = "label", short, long)]
    labels: Vec<KeyValueArgument>,

    /// Start time to be passed into the new notebook (RFC3339). Leave empty to use 60 minutes ago.
    #[clap(long)]
    from: Option<Timestamp>,

    /// End time to be passed into the new notebook (RFC3339). Leave empty to use the current time.
    #[clap(long)]
    to: Option<Timestamp>,

    /// Front matter which should be added to the notebook upon creation. Leave empty to attach no front matter.
    #[clap(long, value_parser = parse_from_str)]
    front_matter: Option<FrontMatter>,

    /// Create the notebook from the given Markdown
    ///
    /// To read the Markdown from a file use `--markdown=$(cat file.md)`
    #[clap(long, short, value_hint = ValueHint::FilePath)]
    markdown: Option<String>,

    /// Output of the notebook
    #[clap(long, short, default_value = "table", value_enum)]
    output: NotebookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_create_command(args: CreateArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url.clone()).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let labels = args.labels.into_iter().map(Into::into).collect();

    let now = Timestamp::now_utc();
    let from = args.from.unwrap_or_else(|| now - 1.hours());
    let to = args.to.unwrap_or(now);

    // Optionally parse the notebook from Markdown
    let notebook = match args.markdown {
        Some(markdown) => {
            let notebook = markdown_to_notebook(&markdown);
            let notebook = serde_json::to_string(&notebook)?;
            serde_json::from_str(&notebook).with_context(|| "Error parsing notebook struct (there is a mismatch between the API client model and the fiberplane notebooks model)")?
        }
        None => NewNotebook::builder()
            .title(String::new())
            .time_range(NewTimeRange::Absolute(TimeRange { from, to }))
            .front_matter(args.front_matter.unwrap_or_else(FrontMatter::new))
            .build(),
    };

    let default_title = if notebook.title.is_empty() {
        "Untitled Notebook".to_string()
    } else {
        notebook.title
    };
    let title = interactive::text_req("Title", args.title, Some(default_title.to_string()))?;

    let notebook = NewNotebook::builder()
        .title(title)
        .time_range(NewTimeRange::Absolute(TimeRange { from, to }))
        .labels(labels)
        .cells(notebook.cells)
        .selected_data_sources(notebook.selected_data_sources)
        .front_matter(notebook.front_matter)
        .build();

    let notebook = notebook_create(&client, workspace_id, notebook).await?;

    match args.output {
        NotebookOutput::Table => {
            info!("Successfully created new notebook");
            let notebook_id = Base64Uuid::parse_str(&notebook.id)?;
            let url = NotebookUrlBuilder::new(workspace_id, notebook_id)
                .base_url(args.base_url)
                .url()?;
            println!("{url}");
            Ok(())
        }
        NotebookOutput::Json => output_json(&notebook),
    }
}

#[derive(Parser)]
pub struct DuplicateArgs {
    /// ID of the source notebook
    #[clap(long, short, env)]
    notebook_id: Option<Base64Uuid>,

    /// Workspace to use (where to clone the notebook)
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Title for the new notebook
    /// Defaults to "Copy of {SOURCE NOTEBOOK TITLE}"
    #[clap(short, long)]
    title: Option<String>,

    /// Output of the notebook
    #[clap(long, short, default_value = "table", value_enum)]
    output: NotebookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_duplicate_command(args: DuplicateArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url.clone()).await?;

    let notebook_id = interactive::notebook_picker_with_prompt(
        "Source Notebook",
        &client,
        args.notebook_id,
        None,
    )
    .await?;

    let source_notebook = notebook_get(&client, notebook_id).await?;

    let workspace_id =
        interactive::workspace_picker_with_prompt("Target workspace", &client, args.workspace_id)
            .await?;
    let new_title = args.title.clone().unwrap_or_else(|| {
        format!(
            "Copy of {}",
            if source_notebook.title.is_empty() {
                "untitled notebook"
            } else {
                &source_notebook.title
            }
        )
    });

    let title = interactive::text_req("Title", args.title, Some(new_title))?;

    let notebook = notebook_duplicate(
        &client,
        notebook_id,
        NotebookCopyDestination::builder()
            .title(title)
            .workspace_id(workspace_id)
            .build(),
    )
    .await?;

    match args.output {
        NotebookOutput::Table => {
            info!("Successfully created new notebook");
            let notebook_id = Base64Uuid::parse_str(&notebook.id)?;
            let url = NotebookUrlBuilder::new(workspace_id, notebook_id)
                .base_url(args.base_url)
                .url()?;
            println!("{url}");
            Ok(())
        }
        NotebookOutput::Json => output_json(&notebook),
    }
}
#[derive(Parser)]
pub struct GetArgs {
    /// ID of the notebook
    #[clap(long, short, env)]
    notebook_id: Option<Base64Uuid>,

    /// Output of the notebook
    #[clap(long, short, default_value = "table", value_enum)]
    output: SingleNotebookOutput,

    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_get_command(args: GetArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;
    let notebook_id = notebook_picker(&client, args.notebook_id, args.workspace_id).await?;

    let notebook = notebook_get(&client, notebook_id).await?;

    match args.output {
        SingleNotebookOutput::Table => output_details(GenericKeyValue::from_notebook(notebook)?),
        SingleNotebookOutput::Json => output_json(&notebook),
        SingleNotebookOutput::Markdown => {
            let notebook = serde_json::to_string(&notebook)?;
            let notebook: notebooks::Notebook = serde_json::from_str(&notebook)?;
            let markdown = notebook_to_markdown(notebook);
            println!("{markdown}");
            Ok(())
        }
    }
}

#[derive(Parser)]
pub struct ListArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the notebook
    #[clap(long, short, default_value = "table", value_enum)]
    output: NotebookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_list_command(args: ListArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let notebooks = notebook_list(&client, workspace_id).await?;

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

#[derive(Parser)]
pub struct SearchArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Labels to search notebooks for (you can specify multiple labels).
    #[clap(name = "label", short, long)]
    labels: Option<Vec<KeyValueArgument>>,

    /// View used to search for notebooks
    view: Option<Name>,

    /// Sort the result according to the following field
    #[clap(long, value_enum)]
    sort_by: Option<NotebookSortFields>,

    /// Sort the result in the following direction
    #[clap(long, value_enum)]
    sort_direction: Option<SortDirection>,

    /// Output of the notebooks
    #[clap(long, short, default_value = "table", value_enum)]
    output: NotebookOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_search_command(args: SearchArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let mut search_params = NotebookSearch::default();

    if let Some(labels) = args.labels {
        if !labels.is_empty() {
            search_params.labels = Some(
                labels
                    .into_iter()
                    .map(|kv| (kv.key, Some(kv.value)))
                    .collect(),
            );
        }
    }

    if search_params.labels.is_none() {
        search_params.view = Some(view_picker(&client, Some(workspace_id), args.view).await?);
    }

    let notebooks = notebook_search(
        &client,
        workspace_id,
        args.sort_by.map(Into::<&str>::into),
        args.sort_direction.map(Into::<&str>::into),
        search_params,
    )
    .await?;

    match args.output {
        NotebookOutput::Table => {
            let notebooks: Vec<NotebookSummaryRow> =
                notebooks.into_iter().map(Into::into).collect();

            output_list(notebooks)
        }
        NotebookOutput::Json => output_json(&notebooks),
    }
}

#[derive(Parser)]
pub struct OpenArgs {
    /// ID of the notebook
    #[clap(long, short, env)]
    notebook_id: Option<Base64Uuid>,

    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_open_command(args: OpenArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url.clone()).await?;
    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let notebook_id = notebook_picker(&client, args.notebook_id, None).await?;

    let url = NotebookUrlBuilder::new(workspace_id, notebook_id)
        .base_url(args.base_url)
        .url()?;

    if open(url.as_str()).is_err() {
        info!("Please go to {} to view the notebook", url);
    }

    Ok(())
}

#[derive(Parser)]
pub struct DeleteArgs {
    /// ID of the notebook
    #[clap(long, short, env)]
    notebook_id: Option<Base64Uuid>,

    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_delete_command(args: DeleteArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;
    let notebook_id = notebook_picker(&client, args.notebook_id, args.workspace_id).await?;

    notebook_delete(&client, notebook_id)
        .await
        .with_context(|| format!("Error deleting notebook {notebook_id}"))?;

    info!(%notebook_id, "Deleted notebook");
    Ok(())
}

#[derive(Parser)]
pub struct AppendCellArgs {
    /// ID of the notebook
    #[clap(long, short, env)]
    notebook_id: Option<Base64Uuid>,

    /// Append a text cell
    #[clap(long, required_unless_present = "code",  conflicts_with_all = &["code"])]
    text: Option<String>,

    /// Append a code cell
    #[clap(long, required_unless_present = "text", conflicts_with_all = &["text"])]
    code: Option<String>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    /// Output type to display
    #[clap(long, short, default_value = "table", value_enum)]
    output: CellOutput,
}

async fn handle_append_cell_command(args: AppendCellArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;
    let notebook_id = notebook_picker(&client, args.notebook_id, None).await?;

    let cell = if let Some(content) = args.text {
        Cell::Text(
            TextCell::builder()
                .content(content)
                .id(String::new())
                .build(),
        )
    } else if let Some(content) = args.code {
        Cell::Code(
            CodeCell::builder()
                .content(content)
                .id(String::new())
                .build(),
        )
    } else {
        unreachable!();
    };

    let cell = notebook_cells_append(&client, notebook_id, None, None, vec![cell])
        .await?
        .pop()
        .ok_or_else(|| anyhow!("Expected a single cell"))?;

    match args.output {
        CellOutput::Json => output_json(&cell),
        CellOutput::Table => {
            info!("Created cell:");
            output_details(GenericKeyValue::from_cell(cell))
        }
    }
}

#[derive(Parser)]
pub struct InsertSnippetArgs {
    /// The workspace to get the snippet from
    #[clap(long, short, env)]
    workspace_id: Option<Base64Uuid>,

    /// The Name of the snippet
    ///
    /// Names must:
    /// - be between 1 and 63 characters long
    /// - start and end with an alphanumeric character
    /// - contain only lowercase alphanumeric ASCII characters and dashes
    ///
    /// Names must be unique within a namespace such as a Workspace.
    snippet_name: Option<Name>,

    /// The notebook to insert the snippet into
    #[clap(long, short, env)]
    notebook_id: Option<Base64Uuid>,

    /// The cell ID after which the snippet should be inserted.
    ///
    /// Note that the cell will be replaced if it is an empty text-based cell.
    #[clap(long, short)]
    cell_id: Option<String>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

pub(crate) async fn handle_insert_snippet_command(args: InsertSnippetArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url.clone()).await?;

    let workspace_id = workspace_picker_with_prompt(
        "Workspace of the snippet and notebook",
        &client,
        args.workspace_id,
    )
    .await?;
    let (workspace_id, snippet_name) =
        snippet_picker(&client, args.snippet_name, Some(workspace_id)).await?;
    let notebook_id = notebook_picker(&client, args.notebook_id, Some(workspace_id)).await?;

    let cells =
        notebook_snippet_insert(&client, notebook_id, &snippet_name, args.cell_id.as_deref())
            .await?;

    let url = NotebookUrlBuilder::new(workspace_id, notebook_id)
        .base_url(args.base_url)
        .cell_id(cells[0].id())
        .url()
        .context("Error constructing notebook URL")?;
    info!("Inserted snippet into notebook: {}", url);

    Ok(())
}

#[derive(Parser)]
pub struct FrontMatterArguments {
    #[clap(subcommand)]
    sub_command: FrontMatterSubCommand,
}

pub async fn handle_front_matter_command(args: FrontMatterArguments) -> Result<()> {
    use FrontMatterSubCommand::*;
    match args.sub_command {
        Update(args) => handle_front_matter_update_command(args).await,
        Clear(args) => handle_front_matter_clear_command(args).await,
    }
}

#[derive(Parser)]
enum FrontMatterSubCommand {
    /// Updates front matter for an existing notebook
    Update(FrontMatterUpdateArguments),

    /// Clears all front matter from an existing notebook
    Clear(FrontMatterClearArguments),
}

#[derive(Parser)]
struct FrontMatterUpdateArguments {
    /// Front matter which should be added. Can override existing keys.
    /// To delete an existing key, set its value to `null`
    #[clap(value_parser = parse_from_str)]
    front_matter: FrontMatter,

    /// Notebook for which front matter should be updated for
    #[clap(long)]
    notebook_id: Option<Base64Uuid>,

    /// Workspace in which the notebook resides in
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_front_matter_update_command(args: FrontMatterUpdateArguments) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let notebook_id = notebook_picker(&client, args.notebook_id, Some(workspace_id)).await?;

    front_matter_update(&client, notebook_id, args.front_matter).await?;

    info!("Successfully updated front matter");
    Ok(())
}

#[derive(Parser)]
struct FrontMatterClearArguments {
    /// Notebook for which front matter should be cleared for
    #[clap(long)]
    notebook_id: Option<Base64Uuid>,

    /// Workspace in which the notebook resides in
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_front_matter_clear_command(args: FrontMatterClearArguments) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let notebook_id = notebook_picker(&client, args.notebook_id, Some(workspace_id)).await?;

    front_matter_delete(&client, notebook_id).await?;

    info!("Successfully cleared front matter");
    Ok(())
}

pub fn parse_from_str(input: &str) -> serde_json::Result<FrontMatter> {
    serde_json::from_str(input)
}

impl GenericKeyValue {
    pub fn from_notebook(notebook: Notebook) -> Result<Vec<GenericKeyValue>> {
        let visibility = notebook.visibility.to_string();

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

        Ok(vec![
            GenericKeyValue::new("Title:", notebook.title),
            GenericKeyValue::new("ID:", notebook.id),
            //GenericKeyValue::new("Created by:", notebook.created_by.name),
            GenericKeyValue::new("Visibility:", visibility),
            GenericKeyValue::new("Updated at:", notebook.updated_at.to_string()),
            GenericKeyValue::new("Created at:", notebook.created_at.to_string()),
            GenericKeyValue::new("Current revision:", notebook.revision.to_string()),
            GenericKeyValue::new("Label:", labels),
        ])
    }

    pub fn from_cell(cell: Cell) -> Vec<GenericKeyValue> {
        let (id, cell_type) = match cell {
            Cell::Text(text_cell) => (text_cell.id, "Text"),
            Cell::Code(code_cell) => (code_cell.id, "Code"),
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

    //#[table(title = "Created by")]
    //pub created_by: String,
    #[table(title = "Visibility")]
    pub visibility: String,

    #[table(title = "Updated at")]
    pub updated_at: String,

    #[table(title = "Created at")]
    pub created_at: String,
}

impl From<NotebookSummary> for NotebookSummaryRow {
    fn from(notebook: NotebookSummary) -> Self {
        let visibility = notebook.visibility.to_string();

        Self {
            id: notebook.id.to_string(),
            title: notebook.title,
            //created_by: notebook.created_by.name,
            visibility,
            updated_at: notebook.updated_at.to_string(),
            created_at: notebook.created_at.to_string(),
        }
    }
}
