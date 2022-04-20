use crate::config::api_client_configuration;
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use anyhow::{anyhow, Context, Error, Result};
use base64uuid::Base64Uuid;
use clap::{ArgEnum, Parser, ValueHint};
use cli_table::Table;
use fiberplane::protocols::core::{self, Cell, HeadingCell, HeadingType, TextCell, TimeRange};
use fiberplane_templates::{notebook_to_template, TemplateExpander};
use fp_api_client::apis::default_api::{
    get_notebook, notebook_create, proxy_data_sources_list, template_create, template_delete,
    template_example_expand, template_example_list, template_expand, template_get, template_list,
    template_update,
};
use fp_api_client::models::{
    NewNotebook, NewTemplate, Notebook, Template, TemplateParameter, TemplateSummary,
    UpdateTemplate,
};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::{env::current_dir, ffi::OsStr, path::PathBuf, str::FromStr};
use tokio::fs;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tracing::{info, warn};
use url::Url;

lazy_static! {
    static ref NOTEBOOK_ID_REGEX: Regex = Regex::from_str("([a-zA-Z0-9_-]{22})$").unwrap();
}

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    /// Initializes a blank template and save it in the current directory as template.jsonnet
    #[clap()]
    Init,

    /// Expand a template into a Fiberplane notebook
    #[clap()]
    Expand(ExpandArguments),

    /// Create a template from an existing Fiberplane notebook
    #[clap()]
    Convert(ConvertArguments),

    /// Create a new template
    #[clap()]
    Create(CreateArguments),

    /// Retrieve a single template
    #[clap()]
    Get(GetArguments),

    /// Remove a template
    #[clap()]
    Remove(RemoveArguments),

    /// List of the templates that have been uploaded to Fiberplane
    #[clap()]
    List(ListArguments),

    /// Update an existing template
    #[clap()]
    Update(UpdateArguments),

    /// Interact with the official example templates
    #[clap(subcommand)]
    Examples(ExamplesSubCommand),
}

#[derive(Parser)]
#[clap(alias = "example")]
enum ExamplesSubCommand {
    /// Expand one of the example templates
    #[clap()]
    Expand(ExpandExampleArguments),

    /// List the example templates
    #[clap()]
    List(ListArguments),

    /// Get a single example templates
    #[clap()]
    Get(GetExampleArguments),
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Init => handle_init_command().await,
        Expand(args) => handle_expand_command(args).await,
        Convert(args) => handle_convert_command(args).await,
        Create(args) => handle_create_command(args).await,
        Remove(args) => handle_delete_command(args).await,
        Get(args) => handle_get_command(args).await,
        List(args) => handle_list_command(args).await,
        Update(args) => handle_update_command(args).await,
        Examples(args) => match args {
            ExamplesSubCommand::Expand(args) => handle_expand_example_command(args).await,
            ExamplesSubCommand::List(args) => handle_list_example_command(args).await,
            ExamplesSubCommand::Get(args) => handle_get_example_command(args).await,
        },
    }
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct TemplateArguments(pub HashMap<String, Value>);

impl FromStr for TemplateArguments {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let args = if let Ok(args) = serde_json::from_str(s) {
            args
        } else {
            let mut args = HashMap::new();
            for kv in s.split([';', ',']) {
                let mut parts = kv.trim().split([':', '=']);
                let key = parts
                    .next()
                    .ok_or_else(|| anyhow!("missing key"))?
                    .to_string();
                let value = Value::String(
                    parts
                        .next()
                        .ok_or_else(|| anyhow!("missing value"))?
                        .to_string(),
                );
                args.insert(key, value);
            }
            args
        };
        Ok(TemplateArguments(args))
    }
}

#[derive(Parser)]
struct ExpandArguments {
    /// ID or URL of a template already uploaded to Fiberplane,
    /// or the path or URL of a template file.
    #[clap(value_hint = ValueHint::AnyPath)]
    template: String,

    /// Values to inject into the template
    ///
    /// Can be passed as a JSON object or as a comma-separated list of key=value pairs
    #[clap()]
    template_arguments: Option<TemplateArguments>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct ConvertArguments {
    /// Notebook ID or URL to convert. Pass - to read the Notebook JSON representation from stdin
    #[clap()]
    notebook: String,

    /// Title of the template (defaults to the notebook title)
    #[clap(long)]
    title: Option<String>,

    /// Description of the template
    #[clap(long, default_value = "")]
    description: String,

    /// Update the given template instead of creating a new one
    #[clap(long)]
    template_id: Option<Base64Uuid>,

    /// By default (if this is not specified), the template will be uploaded to Fiberplane.
    /// If this is specified, save the template to the given file. If specified as "-", print it to stdout.
    #[clap(
        long,
        conflicts_with = "title",
        conflicts_with = "description",
        conflicts_with = "template-id"
    )]
    out: Option<String>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct CreateArguments {
    /// Title of the template
    #[clap(long, required = true)]
    title: String,

    /// Description of the template
    #[clap(long, default_value = "")]
    description: String,

    /// Update the given template instead of creating a new one
    #[clap(long)]
    template_id: Option<Base64Uuid>,

    /// Path or URL of template file to expand
    #[clap(value_hint = ValueHint::AnyPath)]
    template: String,

    /// Output of the template
    #[clap(long, short, default_value = "table", arg_enum)]
    output: TemplateOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct GetArguments {
    /// The ID of the template
    #[clap()]
    template_id: Base64Uuid,

    /// Output of the template
    #[clap(long, short, default_value = "table", arg_enum)]
    output: TemplateOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct RemoveArguments {
    /// The ID of the template
    #[clap()]
    template_id: Base64Uuid,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct ListArguments {
    /// Output of the templates
    #[clap(long, short, default_value = "table", arg_enum)]
    output: TemplateListOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct UpdateArguments {
    /// ID of the template to update
    template_id: Base64Uuid,

    /// Title of the template
    #[clap(long)]
    title: Option<String>,

    /// Description of the template
    #[clap(long)]
    description: Option<String>,

    /// The body of the template
    #[clap(long, conflicts_with = "template-path")]
    template: Option<String>,

    /// Path to the template body file
    #[clap(long, conflicts_with = "template", value_hint = ValueHint::AnyPath)]
    template_path: Option<PathBuf>,

    /// Output of the template
    #[clap(long, short, default_value = "table", arg_enum)]
    output: TemplateOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct ExpandExampleArguments {
    /// Title or ID of the example template to expand
    ///
    /// The title can be passed as a quoted string ("Incident Response") or as kebab-case ("root-cause-analysis")
    #[clap()]
    template: String,

    /// Values to inject into the template
    ///
    /// Can be passed as a JSON object or as a comma-separated list of key=value pairs
    #[clap()]
    template_arguments: Option<TemplateArguments>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct GetExampleArguments {
    /// Title or ID of the example template to expand
    ///
    /// The title can be passed as a quoted string ("Incident Response") or as kebab-case ("root-cause-analysis")
    #[clap()]
    template: String,

    /// Output of the template
    #[clap(long, short, default_value = "table", arg_enum)]
    output: TemplateOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(ArgEnum, Clone)]
enum TemplateOutput {
    /// Output the details of the template as a table (excluding body)
    Table,

    /// Only output the body of the template
    Body,

    /// Output the template as a JSON encoded file
    Json,
}

#[derive(ArgEnum, Clone)]
enum TemplateListOutput {
    /// Output the values as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

async fn handle_init_command() -> Result<()> {
    let notebook = core::NewNotebook {
        title: "Replace me!".to_string(),
        time_range: TimeRange {
            from: 0.0,
            to: 60.0 * 60.0,
        },
        data_sources: BTreeMap::new(),
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

    let mut path = current_dir()?;
    path.push("template.jsonnet");

    fs::write(&path, template).await?;
    info!("Saved template to: {}", path.display());

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

    let config = api_client_configuration(args.config, &args.base_url)
        .await
        .ok();

    let mut expander = TemplateExpander::default();

    // Inject data sources into the template runtime
    let data_sources = if let Some(config) = &config {
        proxy_data_sources_list(config)
            .await
            .with_context(|| "loading proxy data sources")?
    } else {
        Vec::new()
    };
    expander.add_ext_var(
        "PROXY_DATA_SOURCES".to_string(),
        serde_json::to_value(&data_sources)?,
    );

    let template_args = if let Some(args) = args.template_arguments {
        args.0
    } else {
        HashMap::new()
    };
    let notebook = expander
        .expand_template_to_string(template, template_args, false)
        .with_context(|| "expanding template")?;

    let config = config.ok_or_else(|| anyhow!("Must be logged in to create notebook"))?;

    let notebook: NewNotebook = serde_json::from_str(&notebook)
        .with_context(|| "Template did not produce a valid NewNotebook")?;
    let notebook = notebook_create(&config, Some(notebook))
        .await
        .with_context(|| "Error creating notebook")?;
    Ok(notebook)
}

async fn handle_convert_command(args: ConvertArguments) -> Result<()> {
    // Load the notebook from stdin or from the API
    let (notebook, notebook_id, url) = if args.notebook == "-" {
        let mut notebook_json = String::new();
        io::stdin()
            .read_to_string(&mut notebook_json)
            .await
            .with_context(|| "Error reading from stdin")?;
        let notebook: Notebook =
            serde_json::from_str(&notebook_json).with_context(|| "Notebook is invalid")?;
        let url = format!("{}notebook/{}", args.base_url, &notebook.id);
        (notebook_json, notebook.id, url)
    } else {
        let config = api_client_configuration(args.config.clone(), &args.base_url).await?;
        let id = &NOTEBOOK_ID_REGEX
            .captures(&args.notebook)
            .ok_or_else(|| anyhow!("Notebook URL is invalid"))?[1];
        let notebook = get_notebook(&config, id)
            .await
            .with_context(|| "Error fetching notebook")?;
        let notebook = serde_json::to_string(&notebook)?;
        (notebook, id.to_string(), args.notebook)
    };

    // TODO remove the extra (de)serialization when we unify the generated API client
    // types with those in fiberplane-rs
    let mut notebook: core::NewNotebook = serde_json::from_str(&notebook).with_context(|| {
        format!(
            "Error deserializing response as core::NewNotebook: {}",
            notebook
        )
    })?;

    // Add image URLs to ImageCells that were uploaded to the Studio.
    //
    // Images will be loaded from the API when the notebook is created so
    // that the images are stored as files associated with the new notebook.
    for cell in &mut notebook.cells {
        if let Cell::Image(cell) = cell {
            if let (None, Some(file_id)) = (&cell.url, &cell.file_id) {
                cell.url = Some(format!(
                    "{}api/files/{}/{}",
                    args.base_url, notebook_id, file_id
                ));
                cell.file_id = None;
            }
        }
    }

    let notebook_title = notebook.title.clone();
    let template = notebook_to_template(notebook);
    let template = format!(
        "
// This template was generated from the notebook: {}

{}",
        url, template
    );

    match &args.out {
        // Upload the template
        None => {
            let config = api_client_configuration(args.config, &args.base_url).await?;
            match args.template_id {
                Some(template_id) => {
                    let template = UpdateTemplate {
                        title: args.title,
                        description: Some(args.description),
                        body: Some(template),
                    };
                    template_update(&config, &template_id.to_string(), template)
                        .await
                        .with_context(|| format!("Error updating template {}", template_id))?;
                }
                None => {
                    let template = NewTemplate {
                        title: args.title.unwrap_or(notebook_title),
                        description: args.description,
                        body: template,
                    };
                    template_create(&config, template)
                        .await
                        .with_context(|| "Error creating template")?;
                }
            }
        }
        // Write the template to stdout
        Some(path) if path == "-" => {
            io::stdout().write_all(template.as_bytes()).await?;
        }
        // Write the template to a file
        Some(path) => {
            let mut path = PathBuf::from(path);
            // If the given path is a directory, add the filename
            if path.is_dir() {
                path.push("template.jsonnet");
            }
            fs::write(&path, template).await?;
        }
    }

    Ok(())
}

async fn handle_create_command(args: CreateArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let body = load_template(&args.template).await?;
    let template = NewTemplate {
        title: args.title,
        description: args.description,
        body,
    };

    let template = template_create(&config, template)
        .await
        .with_context(|| "Error creating template")?;
    info!("Uploaded template");

    match args.output {
        TemplateOutput::Table => output_details(GenericKeyValue::from_template(template)),
        TemplateOutput::Body => {
            println!("{}", template.body);
            Ok(())
        }
        TemplateOutput::Json => output_json(&template),
    }
}

async fn handle_get_command(args: GetArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let template = template_get(&config, &args.template_id.to_string()).await?;

    match args.output {
        TemplateOutput::Table => output_details(GenericKeyValue::from_template(template)),
        TemplateOutput::Body => {
            println!("{}", template.body);
            Ok(())
        }
        TemplateOutput::Json => output_json(&template),
    }
}

async fn handle_delete_command(args: RemoveArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let template_id = args.template_id;

    template_delete(&config, &template_id.to_string())
        .await
        .with_context(|| format!("Error deleting template {}", template_id))?;

    info!(%template_id, "Deleted template");
    Ok(())
}

async fn handle_list_command(args: ListArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let templates = template_list(&config).await?;

    match args.output {
        TemplateListOutput::Table => {
            let mut templates: Vec<TemplateRow> = templates.into_iter().map(Into::into).collect();

            // Sort by updated at so that the most recent is first
            templates.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

            output_list(templates)
        }
        TemplateListOutput::Json => output_json(&templates),
    }
}

async fn handle_update_command(args: UpdateArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let template_id = &args.template_id.to_string();

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

    let template = template_update(&config, template_id, template)
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

async fn handle_expand_example_command(args: ExpandExampleArguments) -> Result<()> {
    let template = args.template.clone();
    let config = api_client_configuration(args.config, &args.base_url).await?;

    // If the template is passed as an ID, just use it
    // Otherwise, load the list of example templates and find the one with the given title
    let template_id = if Base64Uuid::parse_str(&args.template).is_ok() {
        template
    } else {
        let templates = template_example_list(&config).await?;

        let kebab_case_title = template.to_lowercase().replace(' ', "-");
        let template = templates
            .into_iter()
            .find(|t| t.title.to_lowercase().replace(' ', "-") == kebab_case_title)
            .ok_or_else(|| anyhow!("Example template not found"))?;
        template.id
    };

    let template_arguments = serde_json::to_value(&args.template_arguments.unwrap_or_default())?;
    let notebook = template_example_expand(&config, &template_id, Some(template_arguments)).await?;
    let notebook_url = format!("{}notebook/{}", args.base_url, notebook.id);
    info!("Created notebook: {}", notebook_url);
    Ok(())
}

async fn handle_list_example_command(args: ListArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let templates = template_example_list(&config).await?;

    match args.output {
        TemplateListOutput::Table => {
            let mut templates: Vec<TemplateRow> = templates.into_iter().map(Into::into).collect();

            // Sort by updated at so that the most recent is first
            templates.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

            output_list(templates)
        }
        TemplateListOutput::Json => output_json(&templates),
    }
}

async fn handle_get_example_command(args: GetExampleArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let template = {
        let kebab_case_title = args.template.to_lowercase().replace(' ', "-");
        let template_id = args.template;
        template_example_list(&config)
            .await?
            .into_iter()
            .find(|t| {
                t.id == template_id || t.title.to_lowercase().replace(' ', "-") == kebab_case_title
            })
            .ok_or_else(|| anyhow!("example template not found"))?
    };

    match args.output {
        TemplateOutput::Table => output_details(GenericKeyValue::from_template(template)),
        TemplateOutput::Body => {
            println!("{}", template.body);
            Ok(())
        }
        TemplateOutput::Json => output_json(&template),
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
        ]
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
