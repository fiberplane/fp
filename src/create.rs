use crate::config::api_client_configuration;
use anyhow::Result;
use clap::Parser;
use fp_api_client::apis::default_api;
use fp_api_client::models::{new_notebook, TimeRange};
use reqwest::Url;
use std::fs::File;
use std::io::{stdout, BufReader};
use std::path::PathBuf;

#[derive(Parser)]
pub struct Arguments {
    #[clap(long, short)]
    file: PathBuf,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let file = File::open(args.file)?;
    let buf_reader = BufReader::new(file);
    let input: CreatePayload = serde_yaml::from_reader(buf_reader)?;

    match input.create {
        Create::Notebook(new_notebook) => {
            let new_notebook = fp_api_client::models::NewNotebook::new(
                new_notebook.title,
                TimeRange {
                    from: 0_f64,
                    to: 0_f64,
                },
                vec![],
            );
            let result = default_api::notebook_create(&config, new_notebook).await?;
            serde_yaml::to_writer(stdout(), &result)?;
        } // Create::Proxy(_) => todo!(),
    }

    todo!()
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CreatePayload {
    #[serde(flatten)]
    create: Create,
    // metadata: MetaData
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "apiVersion", content = "spec")]
pub enum Create {
    Notebook(NewNotebook),
    // Proxy(NewProxy),
    // DataSource(NewDataSource),
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct NewNotebook {
    title: String,
}

// ---
// apiVersion: notebook
// spec:
//     title:
//     tags:
