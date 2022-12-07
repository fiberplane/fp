use crate::api_client_configuration;
use crate::interactive::{
    name_req, notebook_picker, select_item, sluggify_str, text_opt, workspace_picker,
};
use crate::output::{output_details, output_json, GenericKeyValue};
use anyhow::{anyhow, Result};
use clap::{Parser, ValueEnum, ValueHint};
use fiberplane::api_client::apis::default_api::{
    notebook_convert_to_snippet, notebook_get, snippet_create,
};
use fiberplane::api_client::models::{NewSnippet, Snippet};
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::names::Name;
use fiberplane::models::notebooks::Cell;
use fiberplane::models::sorting::{SnippetListSortFields, SortDirection};
use std::path::PathBuf;
use url::Url;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    /// Convert cells from an existing notebook into a snippet
    Convert(ConvertArguments),

    /// Create a new snippet
    #[clap(alias = "add")]
    Create(CreateArguments),

    /// Get a snippet
    Get(GetArguments),

    /// Delete a snippet
    #[clap(aliases = &["remove", "rm"])]
    Delete(DeleteArguments),

    /// List of the snippets that have been uploaded to Fiberplane
    List(ListArguments),

    /// Update an existing snippet
    Update(UpdateArguments),

    /// Validate a local snippet
    Validate(ValidateArguments),
}

#[derive(Parser)]
struct ConvertArguments {
    /// The workspace to create the snippet in
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Workspace to create the new snippet in
    /// Notebook ID
    #[clap(long, short, env)]
    notebook_id: Option<Base64Uuid>,

    /// Starting cell of the snippet
    #[clap(long, short)]
    start_cell: Option<String>,

    /// Ending cell of the snippet
    #[clap(long, short)]
    end_cell: Option<String>,

    /// Name of the new snippet (defaults to the notebook title, sluggified)
    ///
    /// You can name an existing snippet to update it.
    ///
    /// Names must:
    /// - be between 1 and 63 characters long
    /// - start and end with an alphanumeric character
    /// - contain only lowercase alphanumeric ASCII characters and dashes
    ///
    /// Names must be unique within a namespace such as a Workspace.
    #[clap(long)]
    snippet_name: Option<Name>,

    /// Description of the snippet
    #[clap(long)]
    description: Option<String>,

    /// Output of the snippet
    #[clap(long, short, default_value = "table", value_enum)]
    output: SnippetOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct CreateArguments {
    /// The workspace to create the snippet in
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Name of the snippet
    ///
    /// Names must:
    /// - be between 1 and 63 characters long
    /// - start and end with an alphanumeric character
    /// - contain only lowercase alphanumeric ASCII characters and dashes
    ///
    /// Names must be unique within a namespace such as a Workspace.
    #[clap(long)]
    snippet_name: Option<Name>,

    /// Description of the snippet
    #[clap(long)]
    description: Option<String>,

    /// Path or URL of to the snippet
    #[clap(value_hint = ValueHint::AnyPath)]
    snippet: String,

    /// Output of the snippet
    #[clap(long, short, default_value = "table", value_enum)]
    output: SnippetOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct GetArguments {
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

    /// Output of the snippet
    #[clap(long, short, default_value = "table", value_enum)]
    output: SnippetOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct DeleteArguments {
    /// The workspace to delete the snippet from
    #[clap(long, short, env)]
    workspace_id: Option<Base64Uuid>,

    /// The Name of the snippet
    snippet_name: Option<Name>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser, Debug)]
struct ListArguments {
    /// The workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the snippets
    #[clap(long, short, default_value = "table", value_enum)]
    output: SnippetListOutput,

    /// Sort the result according to the following field
    #[clap(long, value_enum)]
    sort_by: Option<SnippetListSortFields>,

    /// Sort the result in the following direction
    #[clap(long, value_enum)]
    sort_direction: Option<SortDirection>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct UpdateArguments {
    /// The workspace containing the snippet to be updated
    #[clap(long, short, env)]
    workspace_id: Option<Base64Uuid>,

    /// Name of the snippet to update
    snippet_name: Option<Name>,

    /// New description of the snippet
    #[clap(long)]
    description: Option<String>,

    /// New body of the snippet
    #[clap(long, conflicts_with = "snippet_path")]
    snippet: Option<String>,

    /// Path to the snippet new body file
    #[clap(long, conflicts_with = "snippet", value_hint = ValueHint::AnyPath)]
    snippet_path: Option<PathBuf>,

    /// Output of the snippet
    #[clap(long, short, default_value = "table", value_enum)]
    output: SnippetOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct ValidateArguments {
    /// Path to the snippet file or full snippet body to validate
    #[clap(value_hint = ValueHint::AnyPath)]
    snippet: String,
}

#[derive(ValueEnum, Clone)]
enum SnippetOutput {
    /// Output the details of the snippet as a table (excluding body)
    Table,

    /// Only output the body of the snippet
    Body,

    /// Output the snippet as a JSON encoded file
    Json,
}

#[derive(ValueEnum, Clone, Debug)]
enum SnippetListOutput {
    /// Output the values as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Convert(args) => handle_convert(args).await,
        SubCommand::Create(args) => handle_create(args).await,
        SubCommand::Delete(args) => handle_delete(args).await,
        SubCommand::Get(args) => handle_get(args).await,
        SubCommand::List(args) => handle_list(args).await,
        SubCommand::Update(args) => handle_update(args).await,
        SubCommand::Validate(args) => handle_validate(args).await,
    }
}

async fn handle_convert(args: ConvertArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;
    let notebook_id = notebook_picker(&config, args.notebook_id, Some(workspace_id)).await?;
    let notebook = notebook_get(&config, &notebook_id.to_string()).await?;

    let cells: Vec<Cell> = serde_json::from_str(&serde_json::to_string(&notebook.cells)?)?;
    let display_cells: Vec<String> = cells
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let content = cell.content().unwrap_or_default();
            let content = if content.len() > 100 {
                format!("{}...", &content[..100].replace('\n', "\\n"))
            } else {
                content.to_string()
            };
            format!(
                "{}. \"{}\" ({} cell, ID: {})",
                index,
                content,
                cell.type_str(),
                cell.id()
            )
        })
        .collect();

    // Interactively choose the start and end cell
    let start_cell_index = if let Some(cell_id) = &args.start_cell {
        cells
            .iter()
            .position(|cell| cell.id() == cell_id)
            .ok_or(anyhow!(
                "Could not find cell with ID {} in notebook {}",
                cell_id,
                notebook_id
            ))?
    } else {
        select_item("Start snippet from cell", &display_cells, Some(0))?
    };
    let end_cell_index = if let Some(cell_id) = &args.end_cell {
        cells
            .iter()
            .position(|cell| cell.id() == cell_id)
            .ok_or(anyhow!(
                "Could not find cell with ID {} in notebook {}",
                cell_id,
                notebook_id
            ))?
    } else {
        select_item("End snippet at cell", &display_cells, Some(cells.len() - 1))?
    };
    let start_cell_id = cells[start_cell_index].id();
    let end_cell_id = cells[end_cell_index].id();

    let body = notebook_convert_to_snippet(
        &config,
        &notebook_id.to_string(),
        Some(start_cell_id),
        Some(end_cell_id),
    )
    .await?;

    // Now create the snippet record
    let default_name = sluggify_str(&notebook.title);
    let name = name_req("Snippet name", args.snippet_name, default_name)?;
    let description = text_opt("Description", args.description, None);

    let snippet = NewSnippet {
        name: name.to_string(),
        description,
        body,
    };
    let snippet = snippet_create(&config, &workspace_id.to_string(), snippet).await?;

    match args.output {
        SnippetOutput::Table => output_details(GenericKeyValue::from_snippet(snippet)),
        SnippetOutput::Body => {
            println!("{}", snippet.body);
            Ok(())
        }
        SnippetOutput::Json => output_json(&snippet),
    }
}

async fn handle_create(args: CreateArguments) -> Result<()> {
    todo!()
}

async fn handle_delete(args: DeleteArguments) -> Result<()> {
    todo!()
}

async fn handle_list(args: ListArguments) -> Result<()> {
    todo!()
}

async fn handle_get(args: GetArguments) -> Result<()> {
    todo!()
}

async fn handle_update(args: UpdateArguments) -> Result<()> {
    todo!()
}

async fn handle_validate(args: ValidateArguments) -> Result<()> {
    todo!()
}

impl GenericKeyValue {
    pub fn from_snippet(snippet: Snippet) -> Vec<GenericKeyValue> {
        vec![
            GenericKeyValue::new("Name:", snippet.name),
            GenericKeyValue::new("Description:", snippet.description),
            GenericKeyValue::new("Body:", "omitted (use --output=body)"),
        ]
    }
}
