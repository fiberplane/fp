use crate::config::api_client_configuration;
use anyhow::{anyhow, Context, Error, Result};
use base64uuid::Base64Uuid;
use clap::{Parser, ValueHint};
use fiberplane::protocols::core::{self, Cell, HeadingCell, HeadingType, TextCell, TimeRange};
use fiberplane_templates::{notebook_to_template, TemplateExpander};
use fp_api_client::apis::configuration::Configuration;
use fp_api_client::apis::default_api::{
    get_notebook, notebook_create, proxy_data_sources_list, template_create, template_delete,
    template_expand, template_get, template_update,
};
use fp_api_client::models::{NewNotebook, NewTemplate, Notebook};
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
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Init => handle_init_command().await,
        Expand(args) => handle_expand_command(args).await,
        Convert(args) => handle_convert_command(args).await,
        Upload(args) => handle_upload_command(args).await,
        Delete(args) => handle_delete_command(args).await,
        Get(args) => handle_get_command(args).await,
    }
}

#[derive(Parser)]
enum SubCommand {
    /// Create a blank template and save it in the current directory as template.jsonnet
    #[clap()]
    Init,

    /// Expand a template into a Fiberplane notebook
    #[clap()]
    Expand(ExpandArguments),

    /// Create a template from an existing Fiberplane notebook
    #[clap()]
    Convert(ConvertArguments),

    /// Upload the template to Fiberplane so it can be expanded via the web UI or API
    #[clap()]
    Upload(UploadArguments),

    /// Get the details of a given template
    #[clap(alias = "info")]
    Get(GetArguments),

    /// Delete the given template
    #[clap()]
    Delete(DeleteArguments),
}

#[derive(Parser)]
struct ExpandArguments {
    /// ID or URL of a template already uploaded to Fiberplane,
    /// or the path or URL of a template file.
    #[clap(value_hint = ValueHint::AnyPath)]
    template: String,

    /// Values to inject into the template
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
    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    /// Notebook ID or URL to convert. Pass - to read the Notebook JSON representation from stdin
    #[clap()]
    notebook: String,

    /// Title of the template (defaults to the notebook title)
    #[clap(long)]
    title: Option<String>,

    /// Description of the template
    #[clap(long, default_value = "")]
    description: String,

    /// Whether to make the template publicly accessible.
    /// This means that anyone outside of your organization can
    /// view it, expand it into notebooks, and create triggers
    /// that point to it.
    #[clap(long)]
    public: bool,

    /// Update the given template instead of creating a new one
    #[clap(long)]
    template_id: Option<Base64Uuid>,

    /// By default (if this is not specified), the template will be uploaded to Fiberplane.
    /// If this is specified, save the template to the given file. If specified as "-", print it to stdout.
    #[clap(
        long,
        short,
        conflicts_with = "title",
        conflicts_with = "description",
        conflicts_with = "public",
        conflicts_with = "template-id"
    )]
    out: Option<String>,
}

#[derive(Parser)]
struct UploadArguments {
    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    /// Title of the template
    #[clap(long, required = true)]
    title: String,

    /// Description of the template
    #[clap(long, default_value = "")]
    description: String,

    /// Whether to make the template publicly accessible.
    /// This means that anyone outside of your organization can
    /// view it, expand it into notebooks, and create triggers
    /// that point to it.
    #[clap(long)]
    public: bool,

    /// Update the given template instead of creating a new one
    #[clap(long)]
    template_id: Option<Base64Uuid>,

    /// Path or URL of template file to expand
    #[clap(value_hint = ValueHint::AnyPath)]
    template: String,
}

#[derive(Parser)]
struct GetArguments {
    /// Template ID to delete
    #[clap()]
    template_id: Base64Uuid,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct DeleteArguments {
    /// Template ID to delete
    #[clap()]
    template_id: Base64Uuid,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
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
            let template = NewTemplate {
                title: args.title.unwrap_or(notebook_title),
                description: args.description,
                body: template,
                public: args.public,
            };
            let config = api_client_configuration(args.config, &args.base_url).await?;
            template_update_or_create(&config, args.template_id, template).await?;
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

async fn handle_upload_command(args: UploadArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let body = load_template(&args.template).await?;
    let template = NewTemplate {
        title: args.title,
        description: args.description,
        body,
        public: args.public,
    };
    template_update_or_create(&config, args.template_id, template).await?;
    Ok(())
}

async fn template_update_or_create(
    config: &Configuration,
    template_id: Option<Base64Uuid>,
    template: NewTemplate,
) -> Result<()> {
    if let Some(template_id) = template_id {
        template_update(&config, &template_id.to_string(), template)
            .await
            .with_context(|| format!("Error updating template {}", template_id))?;
        info!("Updated template");
    } else {
        let template = template_create(&config, template)
            .await
            .with_context(|| "Error creating template")?;
        info!("Uploaded template:");
        println!("{}", template.id);
    }
    Ok(())
}

async fn handle_get_command(args: GetArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let template = template_get(&config, &args.template_id.to_string()).await?;
    info!("Title: {}", template.title);
    info!("Description: {}", template.description);
    info!(
        "Visibility: {}",
        if template.public { "Public" } else { "Private" }
    );
    info!("Body:");
    println!("{}", template.body);

    Ok(())
}

async fn handle_delete_command(args: DeleteArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let template_id = args.template_id;
    template_delete(&config, &template_id.to_string())
        .await
        .with_context(|| format!("Error deleting template {}", template_id))?;
    info!("Deleted template");
    Ok(())
}
