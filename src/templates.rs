use std::collections::BTreeMap;

use anyhow::Result;
use clap::Parser;
use fiberplane::protocols::core::{
    Cell, HeadingCell, HeadingType, NewNotebook, TextCell, TimeRange,
};
use fiberplane_templates::{evaluate_template_with_settings, notebook_to_template, Error};

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.subcmd {
        New => handle_new_command().await, // Invoke(args) => handle_invoke_command(args).await,
    }
}

#[derive(Parser)]
pub enum SubCommand {
    #[clap(name = "new", about = "Generate a blank template and print it")]
    New,
    // #[clap(name = "invoke", about = "Invoke a template and print the result")]
    // Invoke(InvokeArguments),
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
