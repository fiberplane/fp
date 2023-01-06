use crate::config::api_client_configuration;
use crate::interactive::{name_opt, text_opt, view_picker, workspace_picker};
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use crate::KeyValueArgument;
use anyhow::Result;
use clap::{Parser, ValueEnum};
use cli_table::Table;
use fiberplane::api_client::{view_delete, view_update, views_create, views_get};
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::labels::Label;
use fiberplane::models::names::Name;
use fiberplane::models::sorting::{SortDirection, ViewSortFields};
use fiberplane::models::views::{NewView, UpdateView, View};
use std::fmt::Display;
use std::path::PathBuf;
use time::format_description::well_known::Rfc3339;
use tracing::info;
use url::Url;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Create(args) => handle_create(args).await,
        SubCommand::List(args) => handle_list(args).await,
        SubCommand::Delete(args) => handle_delete(args).await,
        SubCommand::Update(args) => handle_update(args).await,
    }
}

#[derive(Parser)]
enum SubCommand {
    /// Create a new view
    #[clap(alias = "add")]
    Create(CreateArguments),

    /// List views
    List(ListArguments),

    /// Delete a view
    #[clap(aliases = &["remove", "rm"])]
    Delete(DeleteArguments),

    /// Update an existing view
    Update(UpdateArguments),
}

#[derive(Parser)]
struct CreateArguments {
    /// Name of the view that should be created.
    /// This is distinct from `display_name`, which is not constrained
    name: Option<Name>,

    /// Display name of the view that should be created.
    /// This is distinct from `name`, which is constrained
    display_name: Option<String>,

    /// Description of the view that should be created
    description: Option<String>,

    /// Labels which are associated with this newly created view
    labels: Vec<KeyValueArgument>,

    /// Workspace in which this view lives
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the view
    #[clap(long, short, default_value = "table", value_enum)]
    output: ViewOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_create(args: CreateArguments) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let name = name_opt("Name", args.name, None).unwrap();
    let description = text_opt("Description", args.description.clone(), None).unwrap_or_default();

    let view = NewView {
        name,
        display_name: args.display_name,
        description,
        labels: args.labels.into_iter().map(Into::into).collect(),
    };

    let view = views_create(&client, workspace_id, view).await?;

    info!("Successfully created new view");

    match args.output {
        ViewOutput::Table => output_details(GenericKeyValue::from_view(view)),
        ViewOutput::Json => output_json(&view),
    }
}

#[derive(Parser)]
struct ListArguments {
    /// Workspace to search for views in
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Sort the result according to the following field
    #[clap(long, value_enum)]
    sort_by: Option<ViewSortFields>,

    /// Sort the result in the following direction
    #[clap(long, value_enum)]
    sort_direction: Option<SortDirection>,

    /// Page to display
    #[clap(long)]
    page: Option<i32>,

    /// Amount of views to display per page
    #[clap(long)]
    limit: Option<i32>,

    /// Output of the view
    #[clap(long, short, default_value = "table", value_enum)]
    output: ViewOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_list(args: ListArguments) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;

    let views = views_get(
        &client,
        workspace_id,
        args.sort_by.map(Into::<&str>::into),
        args.sort_direction.map(Into::<&str>::into),
        args.page,
        args.limit,
    )
    .await?;

    match args.output {
        ViewOutput::Table => {
            let rows: Vec<ViewRow> = views.into_iter().map(Into::into).collect();
            output_list(rows)
        }
        ViewOutput::Json => output_json(&views),
    }
}

#[derive(Parser)]
struct DeleteArguments {
    /// Workspace to search delete the view in
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Name of the view which should be deleted
    #[clap(long)]
    view_name: Option<Name>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_delete(args: DeleteArguments) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let view_name = view_picker(&client, args.workspace_id, args.view_name).await?;

    view_delete(&client, workspace_id, &view_name).await?;

    info!("Successfully deleted view");
    Ok(())
}

#[derive(Parser)]
struct UpdateArguments {
    /// Name of the view which should be updated
    #[clap(long)]
    view_name: Option<Name>,

    /// New display name for the view
    #[clap(long, env, required_unless_present_any = ["description", "labels"])]
    display_name: Option<String>,

    /// New description for the view
    #[clap(long, env, required_unless_present_any = ["display_name", "labels"])]
    description: Option<String>,

    /// New labels for the view
    #[clap(long, env, required_unless_present_any = ["display_name", "description"])]
    labels: Option<Vec<KeyValueArgument>>,

    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_update(args: UpdateArguments) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let view_name = view_picker(&client, args.workspace_id, args.view_name).await?;

    view_update(
        &client,
        workspace_id,
        &view_name,
        UpdateView {
            display_name: args.display_name,
            description: args.description,
            labels: args
                .labels
                .map(|labels| labels.into_iter().map(Into::into).collect()),
        },
    )
    .await?;

    info!("Successfully updated view");
    Ok(())
}

#[derive(ValueEnum, Clone)]
enum ViewOutput {
    /// Output the details of the view as a table
    Table,

    /// Output the view as JSON
    Json,
}

#[derive(ValueEnum, Clone, Debug)]
enum ViewListOutput {
    /// Output the result as a table
    Table,

    /// Output the result as a JSON list
    Json,
}

impl GenericKeyValue {
    fn from_view(view: View) -> Vec<Self> {
        vec![
            GenericKeyValue::new("Name:", view.name),
            GenericKeyValue::new("Display Name:", view.display_name),
            GenericKeyValue::new("Description:", view.description),
            GenericKeyValue::new("Labels:", format!("{}", print_labels(&view.labels))),
            GenericKeyValue::new(
                "Created at:",
                view.created_at.format(&Rfc3339).unwrap_or_default(),
            ),
            GenericKeyValue::new(
                "Updated at:",
                view.updated_at.format(&Rfc3339).unwrap_or_default(),
            ),
            GenericKeyValue::new("ID:", view.id.to_string()),
        ]
    }
}

#[derive(Table)]
struct ViewRow {
    #[table(title = "ID")]
    id: String,

    #[table(title = "Name")]
    name: String,

    #[table(title = "Display Name")]
    display_name: String,

    #[table(title = "Description")]
    description: String,

    #[table(title = "Labels", display_fn = "print_labels")]
    labels: Vec<Label>,

    #[table(title = "Created at")]
    created_at: String,

    #[table(title = "Updated at")]
    updated_at: String,
}

impl From<View> for ViewRow {
    fn from(view: View) -> Self {
        Self {
            id: view.id.to_string(),
            name: view.name.to_string(),
            display_name: view.display_name,
            description: view.description,
            labels: view.labels,
            created_at: view.created_at.format(&Rfc3339).unwrap_or_default(),
            updated_at: view.updated_at.format(&Rfc3339).unwrap_or_default(),
        }
    }
}

fn print_labels(input: &Vec<Label>) -> impl Display {
    let mut output = String::new();

    for label in input {
        if !output.is_empty() {
            output.push_str(", ");
        }

        output.push_str(&label.key);

        if !label.value.is_empty() {
            output.push('=');
            output.push_str(&label.value);
        }
    }

    output
}
