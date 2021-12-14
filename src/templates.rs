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
use std::env::{self, current_dir};
use std::path::PathBuf;
use std::str::FromStr;
use tokio::fs;
use tokio::io::{self, AsyncReadExt};
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
        Init => handle_init_command().await,
        Expand(args) => handle_expand_command(args).await,
        Convert(args) => handle_convert_command(args).await,
    }
}

#[derive(Parser)]
enum SubCommand {
    #[clap(
        name = "init",
        about = "Create a blank template and save it in the current directory as template.jsonnet"
    )]
    Init,

    #[clap(
        name = "expand",
        about = "Expand a template into a Fiberplane notebook"
    )]
    Expand(ExpandArguments),

    #[clap(
        name = "convert",
        about = "Create a template from an existing Fiberplane notebook"
    )]
    Convert(ConvertArguments),
}

#[derive(Parser)]
struct ExpandArguments {
    #[clap(
        name = "arg",
        short,
        long,
        about = "Values to inject into the template. Must be in the form name=value. JSON values are supported."
    )]
    args: Vec<TemplateArg>,

    #[clap(name = "template", long, short, about = "Path or URL of template file to expand", value_hint = ValueHint::AnyPath)]
    template: String,

    #[clap(
        name = "create-notebook",
        long,
        about = "Create the notebook on Fiberplane.com and return the URL"
    )]
    create_notebook: bool,

    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,
}

#[derive(Parser)]
struct ConvertArguments {
    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,

    #[clap(
        about = "Notebook URL to convert. Pass \"-\" to read the Notebook JSON representation from stdin"
    )]
    notebook_url: String,
}

struct TemplateArg {
    pub name: String,
    pub value: Value,
}

impl FromStr for TemplateArg {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        if let Some((name, value)) = s.split_once('=') {
            Ok(TemplateArg {
                name: name.to_string(),
                value: serde_json::from_str(value).unwrap_or(Value::String(value.to_string())),
            })
        } else {
            Err(anyhow!(
                "Invalid argument syntax. Must be in the form name=value"
            ))
        }
    }
}

async fn handle_init_command() -> Result<()> {
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

    let mut path = current_dir()?;
    path.push("template.jsonnet");

    fs::write(&path, template).await?;
    println!("Saved template to: {}", path.display());

    Ok(())
}

async fn expand_template(template_path: &str, args: HashMap<String, Value>) -> Result<NewNotebook> {
    let path = PathBuf::from(template_path);
    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) if ext == "jsonnet" => {}
        _ => return Err(anyhow!("Template must be a .jsonnet file")),
    }

    let template = match fs::read_to_string(path).await {
        Ok(template) => template,
        Err(err) => {
            if let Ok(url) = Url::parse(&template_path) {
                reqwest::get(url.as_ref())
                    .await
                    .with_context(|| format!("Error loading template from URL: {}", url))?
                    .text()
                    .await
                    .with_context(|| format!("Error reading remote file as text: {}", url))?
            } else {
                return Err(anyhow!("Unable to load template: {:?}", err));
            }
        }
    };

    evaluate_template(template, &args).with_context(|| "Error evaluating template")
}

async fn handle_expand_command(args: ExpandArguments) -> Result<()> {
    let template_args: HashMap<String, Value> =
        args.args.into_iter().map(|a| (a.name, a.value)).collect();

    let notebook = expand_template(&args.template, template_args).await?;

    if !args.create_notebook {
        println!("{}", serde_json::to_string_pretty(&notebook)?);
    } else {
        let config = api_client_configuration(args.config.as_deref(), &args.base_url).await?;
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
    }
    Ok(())
}

async fn handle_convert_command(args: ConvertArguments) -> Result<()> {
    let (notebook, url) = if args.notebook_url == "-" {
        let mut notebook_json = String::new();
        io::stdin().read_to_string(&mut notebook_json).await?;
        let notebook: Notebook = serde_json::from_str(&notebook_json)?;
        let url = format!("{}/notebook/{}", args.base_url, &notebook.id);
        (notebook, url)
    } else {
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
        (notebook, args.notebook_url)
    };
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
        url, template
    );

    Ok(())
}
