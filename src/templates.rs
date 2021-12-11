use anyhow::{anyhow, Context, Error, Result};
use clap::{Parser, ValueHint};
use fiberplane::protocols::core::{
    Cell, HeadingCell, HeadingType, NewNotebook, TextCell, TimeRange,
};
use fiberplane_templates::{evaluate_template, notebook_to_template};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::path::PathBuf;
use std::str::FromStr;
use tokio::fs;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.subcmd {
        New => handle_new_command().await, // Invoke(args) => handle_invoke_command(args).await,
        Invoke(args) => handle_invoke_command(args).await,
    }
}

#[derive(Parser)]
enum SubCommand {
    #[clap(name = "new", about = "Generate a blank template and print it")]
    New,
    // #[clap(name = "invoke", about = "Invoke a template and print the result")]
    Invoke(InvokeArguments),
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

async fn handle_invoke_command(args: InvokeArguments) -> Result<()> {
    let path = PathBuf::from(args.template);
    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) if ext == "jsonnet" => {}
        _ => return Err(anyhow!("Template must be a .jsonnet file")),
    }

    match fs::read_to_string(path).await {
        Ok(template) => {
            let args: HashMap<String, Value> =
                args.args.into_iter().map(|a| (a.name, a.value)).collect();

            let notebook =
                evaluate_template(template, &args).with_context(|| "Error evaluating template")?;
            println!("{}", serde_json::to_string_pretty(&notebook)?);
            Ok(())
        }
        Err(err) => unimplemented!(),
    }
}
