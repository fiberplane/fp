use crate::config::api_client_configuration;
use crate::interactive::{name_opt, text_opt, view_picker, workspace_picker};
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use crate::utils::clear_or_update;
use crate::KeyValueArgument;
use anyhow::Result;
use clap::{Parser, ValueEnum};
use cli_table::Table;
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::labels::Label;
use fiberplane::models::names::Name;
use fiberplane::models::sorting::{NotebookSortFields, SortDirection, ViewSortFields};
use fiberplane::models::views::{NewView, RelativeTime, TimeUnit, UpdateView, View};
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

    /// The color the resulting view should be displayed as in Fiberplane Studio
    #[clap(long, value_parser = clap::value_parser!(i16).range(0..10))]
    color: i16,

    /// Labels which are associated with this newly created view
    #[clap(long, short)]
    labels: Vec<KeyValueArgument>,

    /// Time range value in either seconds, minutes, hours or days (without suffix).
    /// Used in conjunction with `time_range_unit`
    #[clap(requires = "time_range_unit")]
    time_range_value: Option<i64>,

    /// Time range unit. Used in conjunction with `time_range_value`
    #[clap(requires = "time_range_value", value_enum)]
    time_range_unit: Option<TimeUnit>,

    /// What the notebooks displayed in the view should be sorted by, by default
    #[clap(value_enum)]
    sort_by: Option<NotebookSortFields>,

    /// Sort direction displayed by default when opening the view
    #[clap(value_enum)]
    sort_direction: Option<SortDirection>,

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

    #[clap(from_global)]
    token: Option<String>,
}

async fn handle_create(args: CreateArguments) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let name = name_opt("Name", args.name, None).unwrap();
    let description = text_opt("Description", args.description.clone(), None).unwrap_or_default();

    let time_range = if let Some(unit) = args.time_range_unit {
        Some(
            // .unwrap is safe because value can only exist in conjunction with unit (and vice-versa)
            RelativeTime::new(args.time_range_value.unwrap(), unit),
        )
    } else {
        None
    };

    let mut view = NewView::builder()
        .name(name)
        .description(description)
        .color(args.color)
        .labels(args.labels.into_iter().map(Into::into).collect())
        .build();
    view.display_name = args.display_name;
    view.relative_time = time_range;
    view.sort_by = args.sort_by;
    view.sort_direction = args.sort_direction;

    let view = client.view_create(workspace_id, view).await?;

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

    #[clap(from_global)]
    token: Option<String>,
}

async fn handle_list(args: ListArguments) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;

    let views = client
        .view_list(
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

    #[clap(from_global)]
    token: Option<String>,
}

async fn handle_delete(args: DeleteArguments) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let view_name = view_picker(&client, args.workspace_id, args.view_name).await?;

    client.view_delete(workspace_id, &view_name).await?;

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
    #[clap(long, env, required_unless_present_any = ["display_name", "labels"], conflicts_with = "clear_description")]
    description: Option<String>,

    /// Whenever the existing description should be removed
    #[clap(long, env, conflicts_with = "description")]
    clear_description: bool,

    /// New color for the view
    #[clap(long, env, value_parser = clap::value_parser!(i16).range(0..10))]
    color: Option<i16>,

    /// New labels for the view
    #[clap(long, env, required_unless_present_any = ["display_name", "description"])]
    labels: Option<Vec<KeyValueArgument>>,

    /// New time range value in either seconds, minutes, hours or days (without suffix) for the view.
    /// Used in conjunction with `time_range_unit`
    #[clap(
        long,
        env,
        requires = "time_range_unit",
        conflicts_with = "clear_time_range"
    )]
    time_range_value: Option<i64>,

    /// New time range unit for the view. Used in conjunction with `time_range_value`
    #[clap(
        long,
        env,
        requires = "time_range_value",
        conflicts_with = "clear_time_range"
    )]
    time_range_unit: Option<TimeUnit>,

    /// Whenever the existing time range should be removed
    #[clap(long, env, conflicts_with_all = ["time_range_value", "time_range_unit"])]
    clear_time_range: bool,

    /// What the notebooks displayed in the view should be newly sorted by, by default
    #[clap(long, env, value_enum, conflicts_with = "clear_sort_by")]
    sort_by: Option<NotebookSortFields>,

    /// Whenever the existing sort by should be removed
    #[clap(long, env, conflicts_with = "sort_by")]
    clear_sort_by: bool,

    /// New sort direction displayed by default when opening the view
    #[clap(long, env, value_enum, conflicts_with = "clear_sort_direction")]
    sort_direction: Option<SortDirection>,

    /// Whenever the existing sort direction should be removed
    #[clap(long, env, conflicts_with = "sort_direction")]
    clear_sort_direction: bool,

    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    #[clap(from_global)]
    token: Option<String>,
}

async fn handle_update(args: UpdateArguments) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let view_name = view_picker(&client, args.workspace_id, args.view_name).await?;

    // cant use `.clear_or_update().map(|val| val.map())` because that would result in partial moves :(
    let time_range = if args.clear_time_range {
        Some(None)
    } else if let Some(unit) = args.time_range_unit {
        Some(Some(
            // .unwrap is safe because value can only exist in conjunction with unit (and vice-versa)
            RelativeTime::new(args.time_range_value.unwrap(), unit),
        ))
    } else {
        None
    };

    let mut update = UpdateView::default();
    update.display_name = args.display_name;
    update.description = clear_or_update(args.clear_description, args.description);
    update.color = args.color;
    update.labels = args
        .labels
        .map(|labels| labels.into_iter().map(Into::into).collect());
    update.relative_time = time_range;
    update.sort_by = clear_or_update(args.clear_sort_by, args.sort_by);
    update.sort_direction = clear_or_update(args.clear_sort_direction, args.sort_direction);

    client.view_update(workspace_id, &view_name, update).await?;

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
