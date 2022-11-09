use crate::config::api_client_configuration;
use crate::interactive::{self, workspace_picker};
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use anyhow::{anyhow, bail, Context, Error, Result};
use base64uuid::Base64Uuid;
use clap::{Parser, ValueEnum, ValueHint};
use cli_table::Table;
use fiberplane::protocols::core::{self, Cell, HeadingCell, HeadingType, TextCell};
use fiberplane::sorting::{SortDirection, TemplateListSortFields};
use fp_api_client::apis::configuration::Configuration;
use fp_api_client::apis::default_api::{
    notebook_create, notebook_get, template_create, template_delete, template_expand, template_get,
    template_list, template_update, trigger_create,
};
use fp_api_client::models::{
    NewNotebook, NewTemplate, NewTrigger, Notebook, Template, TemplateParameter, TemplateSummary,
    UpdateTemplate,
};
use fp_templates::{expand_template, notebook_to_template, Error as TemplateError};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, ffi::OsStr, path::PathBuf, str::FromStr};
use tokio::fs;
use tracing::{debug, info, warn};
use url::Url;

lazy_static! {
    pub static ref NOTEBOOK_ID_REGEX: Regex = Regex::from_str("([a-zA-Z0-9_-]{22})$").unwrap();
}

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    /// Initializes a blank template and save it in the current directory as template.jsonnet
    Init(InitArguments),

    /// Expand a template into a Fiberplane notebook
    Expand(ExpandArguments),

    /// Create a template from an existing Fiberplane notebook
    Convert(ConvertArguments),

    /// Create a new template
    #[clap(alias = "add")]
    Create(CreateArguments),

    /// Retrieve a single template
    ///
    /// By default, this returns the template metadata.
    /// To retrieve the full template body, use the --output=body flag
    Get(GetArguments),

    /// Delete a template
    #[clap(aliases = &["remove", "rm"])]
    Delete(DeleteArguments),

    /// List of the templates that have been uploaded to Fiberplane
    List(ListArguments),

    /// Update an existing template
    Update(UpdateArguments),

    /// Validate a local template
    ///
    /// Note that only templates without required parameters can be fully validated.
    Validate(ValidateArguments),
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Init(args) => handle_init_command(args).await,
        Expand(args) => handle_expand_command(args).await,
        Convert(args) => handle_convert_command(args).await,
        Create(args) => handle_create_command(args).await,
        Delete(args) => handle_delete_command(args).await,
        Get(args) => handle_get_command(args).await,
        List(args) => handle_list_command(args).await,
        Update(args) => handle_update_command(args).await,
        Validate(args) => handle_validate_command(args).await,
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct TemplateArguments(pub HashMap<String, Value>);

impl FromStr for TemplateArguments {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let args = if let Ok(args) = serde_json::from_str(s) {
            args
        } else {
            let mut args = HashMap::new();
            for kv in s.split([';', ',']) {
                let (key, value) = kv
                    .trim()
                    .split_once([':', '='])
                    .ok_or_else(|| anyhow!("missing delimiter"))?;

                args.insert(key.to_string(), Value::String(value.to_string()));
            }
            args
        };
        Ok(TemplateArguments(args))
    }
}

#[derive(Parser)]
struct InitArguments {
    #[clap(long, short, default_value = "./template.jsonnet")]
    template_path: PathBuf,
}

#[derive(Parser)]
struct ExpandArguments {
    /// Workspace to use
    #[clap(long, short, env)]
    workspace_id: Option<Base64Uuid>,

    /// ID or URL of a template already uploaded to Fiberplane,
    /// or the path or URL of a template file.
    #[clap(value_hint = ValueHint::AnyPath)]
    template: String,

    /// Values to inject into the template
    ///
    /// Can be passed as a JSON object or as a comma-separated list of key=value pairs
    template_arguments: Option<TemplateArguments>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct ConvertArguments {
    /// The workspace to create the template in
    #[clap(long, short, env)]
    workspace_id: Option<Base64Uuid>,

    /// Workspace to create the new template in
    /// Notebook ID
    #[clap(long, short, env)]
    notebook_id: Option<Base64Uuid>,

    /// Title of the template (defaults to the notebook title)
    #[clap(long)]
    title: Option<String>,

    /// Description of the template
    #[clap(long)]
    description: Option<String>,

    /// Update the given template instead of creating a new one
    #[clap(long)]
    template_id: Option<Base64Uuid>,

    /// Create a trigger for the template
    ///
    /// Triggers are Webhook URLs that allow you to expand the template
    /// from an external service such as an alert system.
    #[clap(long)]
    create_trigger: Option<bool>,

    /// Output of the template
    #[clap(long, short, default_value = "table", value_enum)]
    output: TemplateOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct CreateArguments {
    /// The workspace to create the template in
    #[clap(long, short, env)]
    workspace_id: Option<Base64Uuid>,

    /// Title of the template
    #[clap(long)]
    title: Option<String>,

    /// Description of the template
    #[clap(long)]
    description: Option<String>,

    /// Path or URL of to the template
    #[clap(value_hint = ValueHint::AnyPath)]
    template: String,

    /// Create a trigger for the template
    ///
    /// Triggers are Webhook URLs that allow you to expand the template
    /// from an external service such as an alert system.
    #[clap(long)]
    create_trigger: Option<bool>,

    /// Output of the template
    #[clap(long, short, default_value = "table", value_enum)]
    output: TemplateOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct GetArguments {
    /// The ID of the template
    template_id: Option<Base64Uuid>,

    /// Output of the template
    #[clap(long, short, default_value = "table", value_enum)]
    output: TemplateOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct DeleteArguments {
    /// The ID of the template
    template_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser, Debug)]
struct ListArguments {
    /// The workspace to use
    #[clap(long, short, env)]
    workspace_id: Option<Base64Uuid>,

    /// Output of the templates
    #[clap(long, short, default_value = "table", value_enum)]
    output: TemplateListOutput,

    /// Sort the result according to the following field
    #[clap(long, value_enum)]
    sort_by: Option<TemplateListSortFields>,

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
    /// ID of the template to update
    template_id: Option<Base64Uuid>,

    /// Title of the template
    #[clap(long)]
    title: Option<String>,

    /// Description of the template
    #[clap(long)]
    description: Option<String>,

    /// The body of the template
    #[clap(long, conflicts_with = "template_path")]
    template: Option<String>,

    /// Path to the template body file
    #[clap(long, conflicts_with = "template", value_hint = ValueHint::AnyPath)]
    template_path: Option<PathBuf>,

    /// Output of the template
    #[clap(long, short, default_value = "table", value_enum)]
    output: TemplateOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct ValidateArguments {
    /// Path to the template file or full template body to validate
    #[clap(value_hint = ValueHint::AnyPath)]
    template: String,

    /// Optional values to inject into the template
    ///
    /// Can be passed as a JSON object or as a comma-separated list of key=value pairs
    template_arguments: Option<TemplateArguments>,
}

#[derive(ValueEnum, Clone)]
enum TemplateOutput {
    /// Output the details of the template as a table (excluding body)
    Table,

    /// Only output the body of the template
    Body,

    /// Output the template as a JSON encoded file
    Json,
}

#[derive(ValueEnum, Clone, Debug)]
enum TemplateListOutput {
    /// Output the values as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

async fn handle_init_command(args: InitArguments) -> Result<()> {
    let notebook = core::NewNotebook {
        title: "Replace me!".to_string(),
        time_range: core::NewTimeRange::Relative(core::RelativeTimeRange { minutes: -60 }),
        selected_data_sources: Default::default(),
        cells: vec![
            Cell::Heading(HeadingCell {
                id: "1".to_string(),
                heading_type: HeadingType::H1,
                content: "This is a section".to_string(),
                read_only: None,
                formatting: None,
            }),
            Cell::Text(TextCell {
                id: "2".to_string(),
                content: "You can add any types of cells and pre-fill content".to_string(),
                read_only: None,
                formatting: None,
            }),
        ],
        labels: Vec::new(),
    };
    let template = notebook_to_template(notebook);

    if args.template_path.exists() {
        bail!(
            "File already exists at path: {}",
            args.template_path.display()
        );
    }

    fs::write(&args.template_path, template).await?;
    info!("Saved template to: {}", args.template_path.display());

    Ok(())
}

/// Load the template file, either from a server if the
/// template_path is an HTTPS URL, or from a local file
async fn load_template(template_path: &str) -> Result<String> {
    if template_path.starts_with("https://") || template_path.starts_with("http://") {
        if template_path.starts_with("http://") {
            warn!("Templates can be manually expanded from HTTP URLs but triggers must use HTTPS URLs");
        }
        reqwest::get(template_path)
            .await
            .with_context(|| format!("loading template from URL: {}", template_path))?
            .error_for_status()
            .with_context(|| format!("loading template from URL: {}", template_path))?
            .text()
            .await
            .with_context(|| format!("reading remote file as text: {}", template_path))
    } else {
        let path = PathBuf::from(template_path);
        if path.extension() == Some(OsStr::new("jsonnet")) {
            fs::read_to_string(path)
                .await
                .with_context(|| "reading jsonnet file")
        } else {
            Err(anyhow!("Template must be a .jsonnet file"))
        }
    }
}

async fn handle_expand_command(args: ExpandArguments) -> Result<()> {
    let base_url = args.base_url.clone();
    let template_url_base = base_url.join("templates/")?;

    // First, check if the template is the ID of an uploaded template
    let notebook = if let Ok(template_id) = Base64Uuid::parse_str(&args.template) {
        expand_template_api(args, template_id).await
    } else if let Some(template_id) = args.template.strip_prefix(template_url_base.as_str()) {
        // Next, check if it is a URL of an uploaded template
        let template_id = Base64Uuid::parse_str(template_id)
            .with_context(|| "Error parsing template ID from URL")?;
        expand_template_api(args, template_id).await
    } else {
        // Otherwise, treat the template as a local path or URL of a template file
        expand_template_file(args).await
    }?;

    let notebook_url = format!("{}notebook/{}", base_url, notebook.id);
    info!("Created notebook: {}", notebook_url);
    Ok(())
}

/// Expand a template that has already been uploaded to Fiberplane
async fn expand_template_api(args: ExpandArguments, template_id: Base64Uuid) -> Result<Notebook> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let template_arguments = serde_json::to_value(&args.template_arguments.unwrap_or_default())?;
    let notebook = template_expand(&config, &template_id.to_string(), Some(template_arguments))
        .await
        .with_context(|| format!("Error expanding template: {}", template_id))?;
    Ok(notebook)
}

/// Expand a template that is either a local file or one hosted remotely
async fn expand_template_file(args: ExpandArguments) -> Result<Notebook> {
    let template = load_template(&args.template).await?;

    let config = api_client_configuration(args.config, &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;

    let template_args = if let Some(args) = args.template_arguments {
        args.0
    } else {
        HashMap::new()
    };

    let notebook =
        expand_template(template, template_args).with_context(|| "expanding template")?;

    // Convert to a string and back because the API client
    // has a different model struct than the Rust core types
    let notebook: NewNotebook = serde_json::to_string(&notebook)
        .and_then(|s| serde_json::from_str(&s))
        .with_context(|| "Error converting notebook to API client NewNotebook type")?;

    let notebook = notebook_create(&config, &workspace_id.to_string(), notebook)
        .await
        .with_context(|| "Error creating notebook")?;
    Ok(notebook)
}

async fn handle_convert_command(args: ConvertArguments) -> Result<()> {
    // Load the notebook
    let config = api_client_configuration(args.config.clone(), &args.base_url).await?;
    let workspace_id = workspace_picker(&config, args.workspace_id).await?;
    let notebook_id =
        interactive::notebook_picker(&config, args.notebook_id, Some(workspace_id)).await?;

    let notebook = notebook_get(&config, &notebook_id.to_string())
        .await
        .with_context(|| "Error fetching notebook")?;
    let notebook_id = notebook.id.clone();

    // Convert the notebook from the type returned by the API to the core type
    let mut notebook: core::NewNotebook = serde_json::to_string(&notebook)
        .and_then(|s| serde_json::from_str(&s))
        .with_context(|| "Error converting from API client model to core model")?;
    let notebook_title = notebook.title.clone();

    // Add image URLs to ImageCells that were uploaded to the Studio.
    //
    // Images will be loaded from the API when the notebook is created so
    // that the images are stored as files associated with the new notebook.
    for cell in &mut notebook.cells {
        if let Cell::Image(cell) = cell {
            if let (None, Some(file_id)) = (&cell.url, &cell.file_id) {
                cell.url = Some(format!(
                    "{}api/notebooks/{}/files/{}",
                    args.base_url, notebook_id, file_id
                ));
                cell.file_id = None;
            }
        }
    }

    // TODO we should use the API instead.
    // However, the generated API client doesn't currently support routes that return
    // plain strings (rather than JSON objects) so we'll convert it locally instead
    let template = notebook_to_template(notebook);

    let title = interactive::text_opt("Template Title", args.title, Some(notebook_title)).unwrap();
    let description = interactive::text_opt("Template Description", args.description, None);

    // Create or update the template
    let (template, trigger_url) = if let Some(template_id) = args.template_id {
        let template = UpdateTemplate {
            title: Some(title),
            description,
            body: Some(template),
        };
        let template = template_update(&config, &template_id.to_string(), template)
            .await
            .with_context(|| format!("Error updating template {}", template_id))?;
        info!("Updated template");
        (template, None)
    } else {
        let template = NewTemplate {
            title,
            description: description.unwrap_or_default(),
            body: template,
        };
        create_template_and_trigger(&config, workspace_id, args.create_trigger, template).await?
    };

    match args.output {
        TemplateOutput::Table => output_details(GenericKeyValue::from_template_and_trigger_url(
            template,
            trigger_url,
        )),
        TemplateOutput::Body => {
            println!("{}", template.body);
            Ok(())
        }
        TemplateOutput::Json => output_json(&template),
    }
}

async fn handle_create_command(args: CreateArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let workspace_id = workspace_picker(&config, args.workspace_id).await?;
    let title = interactive::text_req("Title", args.title, Some("".to_owned()))?;
    let description =
        interactive::text_req("Description", args.description.clone(), Some("".to_owned()))?;

    let body = load_template(&args.template).await?;
    let template = NewTemplate {
        title,
        description,
        body,
    };

    let (template, trigger_url) =
        create_template_and_trigger(&config, workspace_id, args.create_trigger, template).await?;

    match args.output {
        TemplateOutput::Table => output_details(GenericKeyValue::from_template_and_trigger_url(
            template,
            trigger_url,
        )),
        TemplateOutput::Body => {
            println!("{}", template.body);
            Ok(())
        }
        TemplateOutput::Json => output_json(&template),
    }
}

async fn handle_get_command(args: GetArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let template_id = interactive::template_picker(&config, args.template_id, None).await?;
    let template = template_get(&config, &template_id.to_string()).await?;

    match args.output {
        TemplateOutput::Table => output_details(GenericKeyValue::from_template(template)),
        TemplateOutput::Body => {
            println!("{}", template.body);
            Ok(())
        }
        TemplateOutput::Json => output_json(&template),
    }
}

async fn handle_delete_command(args: DeleteArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let template_id = interactive::template_picker(&config, args.template_id, None).await?;

    template_delete(&config, &template_id.to_string())
        .await
        .with_context(|| format!("Error deleting template {}", template_id))?;

    info!(%template_id, "Deleted template");
    Ok(())
}

async fn handle_list_command(args: ListArguments) -> Result<()> {
    debug!("handle list command");

    let config = api_client_configuration(args.config, &args.base_url).await?;

    let workspace_id = interactive::workspace_picker(&config, args.workspace_id).await?;

    let templates = template_list(
        &config,
        &workspace_id.to_string(),
        args.sort_by.map(Into::into),
        args.sort_direction.map(Into::into),
    )
    .await?;

    match args.output {
        TemplateListOutput::Table => {
            let templates: Vec<TemplateRow> = templates.into_iter().map(Into::into).collect();
            output_list(templates)
        }
        TemplateListOutput::Json => output_json(&templates),
    }
}

async fn handle_update_command(args: UpdateArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let template_id = interactive::template_picker(&config, args.template_id, None).await?;

    let body = if let Some(template) = args.template {
        Some(template)
    } else if let Some(template_path) = args.template_path {
        Some(
            fs::read_to_string(&template_path)
                .await
                .with_context(|| format!("Unable to read template from: {:?}", template_path))?,
        )
    } else {
        None
    };

    let template = UpdateTemplate {
        title: args.title,
        description: args.description,
        body,
    };

    let template = template_update(&config, &template_id.to_string(), template)
        .await
        .with_context(|| format!("Error updating template {}", template_id))?;
    info!("Updated template");

    match args.output {
        TemplateOutput::Table => output_details(GenericKeyValue::from_template(template)),
        TemplateOutput::Body => {
            println!("{}", template.body);
            Ok(())
        }
        TemplateOutput::Json => output_json(&template),
    }
}

async fn handle_validate_command(args: ValidateArguments) -> Result<()> {
    let template = if let Ok(path) = PathBuf::from_str(&args.template) {
        fs::read_to_string(path).await?
    } else {
        args.template
    };
    let params = args.template_arguments.unwrap_or_default();

    match expand_template(&template, params.0) {
        Ok(_) => {
            info!("Template is valid");
            Ok(())
        }
        Err(TemplateError::MissingArgument(param)) => {
            bail!(
                "Cannot validate template because it has required parameters.\n\n\
            You can either provide example arguments to this command or \
            add a default value for the parameter. \
            For example: function({}='default value') {{...}}",
                param
            );
        }
        Err(TemplateError::InvalidOutput(err)) => {
            bail!("Template did not produce a valid Notebook: {:?}", err)
        }
        Err(TemplateError::Evaluation(err)) => {
            bail!("Error evaluating template: {}", err)
        }
    }
}

#[derive(Table)]
pub struct TemplateRow {
    #[table(title = "Title")]
    pub title: String,

    #[table(title = "ID")]
    pub id: String,

    #[table(title = "Updated at")]
    pub updated_at: String,

    #[table(title = "Created at")]
    pub created_at: String,
}

impl From<TemplateSummary> for TemplateRow {
    fn from(template: TemplateSummary) -> Self {
        Self {
            id: template.id,
            title: template.title,
            updated_at: template.updated_at,
            created_at: template.created_at,
        }
    }
}

impl From<Template> for TemplateRow {
    fn from(template: Template) -> Self {
        Self {
            id: template.id,
            title: template.title,
            updated_at: template.updated_at,
            created_at: template.created_at,
        }
    }
}

impl GenericKeyValue {
    pub fn from_template(template: Template) -> Vec<GenericKeyValue> {
        vec![
            GenericKeyValue::new("Title:", template.title),
            GenericKeyValue::new("ID:", template.id),
            GenericKeyValue::new(
                "Parameters:",
                format_template_parameters(template.parameters),
            ),
            GenericKeyValue::new("Body:", "omitted (use --output=body)"),
        ]
    }

    pub fn from_template_and_trigger_url(
        template: Template,
        trigger_url: Option<String>,
    ) -> Vec<GenericKeyValue> {
        let mut rows = Self::from_template(template);
        if let Some(trigger_url) = trigger_url {
            rows.push(GenericKeyValue::new("Trigger URL:", trigger_url));
        }
        rows
    }
}

fn format_template_parameters(parameters: Vec<TemplateParameter>) -> String {
    if parameters.is_empty() {
        return String::from("(none)");
    }

    let mut result: Vec<String> = vec![];
    for parameter in parameters {
        match parameter {
            TemplateParameter::StringTemplateParameter {
                name,
                default_value,
            } => {
                result.push(format!(
                    "{}: string (default: \"{}\")",
                    name,
                    default_value.unwrap_or_default()
                ));
            }
            TemplateParameter::NumberTemplateParameter {
                name,
                default_value,
            } => {
                result.push(format!(
                    "{}: number (default: {})",
                    name,
                    default_value.unwrap_or_default()
                ));
            }
            TemplateParameter::BooleanTemplateParameter {
                name,
                default_value,
            } => {
                result.push(format!(
                    "{}: boolean (default: {})",
                    name,
                    default_value.unwrap_or_default()
                ));
            }
            TemplateParameter::ArrayTemplateParameter {
                name,
                default_value,
            } => {
                result.push(format!(
                    "{}: array (default: {})",
                    name,
                    serde_json::to_string(&default_value).unwrap()
                ));
            }
            TemplateParameter::ObjectTemplateParameter {
                name,
                default_value,
            } => {
                result.push(format!(
                    "{}: object (default: {})",
                    name,
                    serde_json::to_string(&default_value).unwrap()
                ));
            }
            TemplateParameter::UnknownTemplateParameter { name } => {
                result.push(format!("{}: (type unknown)", name));
            }
        };
    }

    result.join("\n")
}

async fn create_template_and_trigger(
    config: &Configuration,
    workspace_id: Base64Uuid,
    create_trigger: Option<bool>,
    template: NewTemplate,
) -> Result<(Template, Option<String>)> {
    let create_trigger = interactive::bool_req(
        "Create a Trigger (Webhook URL) for this template?",
        create_trigger,
        false,
    );

    let template = template_create(config, &workspace_id.to_string(), template)
        .await
        .with_context(|| "Error creating template")?;
    info!("Uploaded template");

    let trigger_url = if create_trigger {
        let trigger = trigger_create(
            config,
            &workspace_id.to_string(),
            NewTrigger {
                title: format!("{} Trigger", &template.title),
                template_id: template.id.clone(),
                default_arguments: None,
            },
        )
        .await
        .context("Error creating trigger")?;
        let trigger_url = format!(
            "{}/api/triggers/{}/{}",
            config.base_path,
            trigger.id,
            trigger
                .secret_key
                .ok_or_else(|| anyhow!("Trigger creation did not return the secret key"))?
        );
        Some(trigger_url)
    } else {
        None
    };

    Ok((template, trigger_url))
}
