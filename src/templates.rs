use crate::config::api_client_configuration;
use anyhow::{anyhow, Context, Error, Result};
use clap::{Parser, ValueHint};
use fiberplane::protocols::core::{
    Cell, HeadingCell, HeadingType, NewNotebook, Notebook, TextCell, TimeRange,
};
use fiberplane_templates::{evaluate_template, notebook_to_template};
use lazy_static::lazy_static;
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::path::PathBuf;
use std::str::FromStr;
use tokio::fs;
use url::Url;

lazy_static! {
    static ref NOTEBOOK_ID_REGEX: Regex = Regex::from_str("[a-zA-Z0-9]+$").unwrap();
}

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.subcmd {
        New => handle_new_command().await,
        Invoke(args) => handle_invoke_command(args).await,
        CreateNotebook(args) => handle_create_notebook_command(args).await,
        FromNotebook(args) => handle_from_notebook_command(args).await,
    }
}

#[derive(Parser)]
enum SubCommand {
    #[clap(name = "new", about = "Generate a blank template and print it")]
    New,

    #[clap(name = "invoke", about = "Invoke a template and print the result")]
    Invoke(InvokeArguments),

    #[clap(
        name = "create-notebook",
        about = "Invoke the template and create a Fiberplane notebook from it"
    )]
    CreateNotebook(CreateNotebookArguments),

    #[clap(
        name = "from-notebook",
        about = "Create a template from an existing Fiberplane notebook"
    )]
    FromNotebook(FromNotebookArguments),
}

#[derive(Parser)]
struct InvokeArguments {
    #[clap(
        name = "arg",
        short,
        long,
        about = "Values to inject into the template. Must be in the form name=value. JSON values are supported."
    )]
    args: Vec<TemplateArg>,

    #[clap(name = "template", long, short, about = "Path or URL of template file to invoke", value_hint = ValueHint::AnyPath)]
    template: String,
}

#[derive(Parser)]
struct CreateNotebookArguments {
    #[clap(flatten)]
    invoke_args: InvokeArguments,

    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,
}

#[derive(Parser)]
struct FromNotebookArguments {
    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,

    #[clap(about = "Notebook URL to convert")]
    notebook_url: String,
}

struct TemplateArg {
    pub name: String,
    pub value: Value,
}

impl FromStr for TemplateArg {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let out: Vec<_> = s.split('=').collect();
        let (name, value) = match out.len() {
            1 => (
                out[0].to_string(),
                env::var(out[0])
                    .or(Err(anyhow!(format!("Missing env var: \"{}\" (if you did not mean to pass this as an env var, you should write it in the form: name=value", out[0]))))?,
            ),
            2 => (out[0].to_string(), out[1].to_string()),
            _ => {
                return Err(anyhow!(
                    "Invalid argument syntax. Must be in the form name=value"
                ))
            }
        };
        Ok(TemplateArg {
            name,
            value: serde_json::from_str(&value).unwrap_or_else(|_| Value::String(value)),
        })
    }
}

async fn handle_new_command() -> Result<()> {
    let notebook = NewNotebook {
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
            }),
            Cell::Text(TextCell {
                id: "2".to_string(),
                content: "You can add any types of cells and pre-fill content".to_string(),
                read_only: None,
            }),
        ],
    };
    let template = notebook_to_template(notebook);
    println!(
        "// This is a Fiberplane Template. Save it to a file
// with the extension \".jsonnet\" and edit it
// however you like!
    
{}",
        template
    );
    Ok(())
}

async fn invoke_template(args: InvokeArguments) -> Result<NewNotebook> {
    let path = PathBuf::from(&args.template);
    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) if ext == "jsonnet" => {}
        _ => return Err(anyhow!("Template must be a .jsonnet file")),
    }

    let template = match fs::read_to_string(path).await {
        Ok(template) => template,
        Err(err) => {
            if let Ok(url) = Url::parse(&args.template) {
                reqwest::get(url.as_ref())
                    .await
                    .with_context(|| format!("Error loading template from URL: {}", url))?
                    .text()
                    .await
                    .with_context(|| format!("Error reading remote file as text"))?
            } else {
                return Err(anyhow!("Unable to load template: {:?}", err));
            }
        }
    };

    let args: HashMap<String, Value> = args.args.into_iter().map(|a| (a.name, a.value)).collect();

    evaluate_template(template, &args).with_context(|| "Error evaluating template")
}

async fn handle_invoke_command(args: InvokeArguments) -> Result<()> {
    let notebook = invoke_template(args).await?;
    println!("{}", serde_json::to_string_pretty(&notebook)?);
    Ok(())
}

async fn handle_create_notebook_command(args: CreateNotebookArguments) -> Result<()> {
    let config = api_client_configuration(args.config.as_deref(), &args.base_url).await?;
    let notebook = invoke_template(args.invoke_args).await?;
    // TODO use generated API client

    let mut url = Url::parse(&config.base_path)?;
    {
        url.path_segments_mut()
            .map_err(|_| anyhow!("Cannot create API URL"))?
            .push("api")
            .push("notebooks");
    }

    let notebook: Notebook = config
        .client
        .post(url)
        .bearer_auth(
            config
                .oauth_access_token
                .or(config.bearer_access_token)
                .unwrap_or_default(),
        )
        .body(serde_json::to_string(&notebook)?)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let notebook_url = format!("{}/notebook/{}", config.base_path, notebook.id);
    println!("Created notebook: {}", notebook_url);

    Ok(())
}

async fn handle_from_notebook_command(args: FromNotebookArguments) -> Result<()> {
    let config = api_client_configuration(args.config.as_deref(), &args.base_url).await?;

    let notebook_id = &NOTEBOOK_ID_REGEX.captures(&args.notebook_url).unwrap()[0];
    let mut url = Url::parse(&config.base_path)?;
    {
        url.path_segments_mut()
            .map_err(|_| anyhow!("Cannot create API URL"))?
            .push("api")
            .push("notebooks")
            .push(notebook_id);
    }

    // TODO use generated API client
    let notebook: Notebook = config
        .client
        .get(url)
        .bearer_auth(
            config
                .oauth_access_token
                .or(config.bearer_access_token)
                .unwrap_or_default(),
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let notebook = NewNotebook {
        title: notebook.title,
        cells: notebook.cells,
        data_sources: notebook.data_sources,
        time_range: notebook.time_range,
    };
    let template = notebook_to_template(notebook);
    println!(
        "
// This template was generated from the notebook: {}

{}",
        args.notebook_url, template
    );

    Ok(())
}
