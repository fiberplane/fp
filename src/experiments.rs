use crate::config::api_client_configuration;
use crate::fp_urls::NotebookUrlBuilder;
use crate::interactive;
use crate::output::{output_details, output_json, GenericKeyValue};
use crate::templates::NOTEBOOK_ID_REGEX;
use anyhow::{anyhow, Context, Result};
use clap::{Parser, ValueEnum};
use directories::ProjectDirs;
use fiberplane::api_client::{notebook_cells_append, notebook_get, profile_get};
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::markdown::notebook_to_markdown;
use fiberplane::models::formatting::{Annotation, AnnotationWithOffset, Mention};
use fiberplane::models::notebooks::{Cell, ProviderCell, TextCell};
use fiberplane::models::{formatting, notebooks};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Error, Response, Server, StatusCode};
use lazy_static::lazy_static;
use qstring::QString;
use regex::{Regex, Replacer};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::{convert::Infallible, sync::Arc};
use std::{fmt::Write, io::ErrorKind, net::IpAddr, path::PathBuf, str::FromStr};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::fs;
use tracing::{debug, error, info, warn};
use url::Url;

lazy_static! {
    pub static ref NOTEBOOK_URL_REGEX: Regex =
        Regex::from_str(r"http\S+[/]notebooks?[/]\S*([a-zA-Z0-9_-]{22})\b").unwrap();
}

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    /// Append a message to the given notebook
    Message(MessageArgs),

    /// Starting with the given notebook, recursively crawl all linked notebooks
    /// and save them to the given directory as Markdown
    Crawl(CrawlArgs),

    /// Open Prometheus graphs in a given notebook
    PrometheusGraphToNotebook(PrometheusGraphToNotebookArgs),

    /// Panics the CLI in order to test out `human-panic`
    #[clap(hide = true)]
    #[doc(hidden)]
    Panic,
}

#[derive(Parser)]
struct MessageArgs {
    /// The notebook to append the message to
    #[clap(long, short, env)]
    notebook_id: Option<Base64Uuid>,

    /// The message to append
    message: Vec<String>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    /// Output type to display
    #[clap(long, short, default_value = "table", value_enum)]
    output: MessageOutput,
}

#[derive(Parser)]
struct CrawlArgs {
    #[clap(long, short, env)]
    notebook_id: Option<Base64Uuid>,

    #[clap(long, default_value = "10")]
    concurrent_downloads: u8,

    #[clap(long, short)]
    out_dir: PathBuf,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
struct PrometheusGraphToNotebookArgs {
    #[clap(long, short, env)]
    notebook_id: Option<Base64Uuid>,

    /// Server port number
    #[clap(long, short, env, default_value = "9090")]
    port: u16,

    /// Hostname to listen on
    #[clap(long, short = 'H', env, default_value = "127.0.0.1")]
    listen_host: IpAddr,

    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(ValueEnum, Clone)]
enum MessageOutput {
    /// Output the result as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Message(args) => handle_message_command(args).await,
        SubCommand::Crawl(args) => handle_crawl_command(args).await,
        SubCommand::PrometheusGraphToNotebook(args) => {
            handle_prometheus_redirect_command(args).await
        }
        SubCommand::Panic => panic!("manually created panic called by `fpx experiments panic`"),
    }
}

async fn handle_message_command(args: MessageArgs) -> Result<()> {
    let client = api_client_configuration(args.config, args.base_url).await?;
    let notebook_id = interactive::notebook_picker(&client, args.notebook_id, None).await?;
    let mut cache = Cache::load().await?;

    // If we don't already know the user name, load it from the API and save it
    let (user_id, name) = match (cache.user_id, cache.user_name) {
        (Some(user_id), Some(user_name)) => (Base64Uuid::from_str(&user_id)?, user_name),
        _ => {
            let user = profile_get(&client)
                .await
                .with_context(|| "Error getting user profile")?;
            cache.user_name = Some(user.name.clone());
            cache.user_id = Some(user.id.to_string());
            cache.save().await?;
            (user.id, user.name)
        }
    };

    let timestamp_prefix = format!("💬 {} ", OffsetDateTime::now_utc().format(&Rfc3339)?);
    // Note we don't use .len() because it returns the byte length as opposed to the char length (which is different because of the emoji)
    let mention_start = timestamp_prefix.chars().count();
    let prefix = format!("{timestamp_prefix}@{name}:  ");
    let content = format!("{}{}", prefix, args.message.join(" "));

    let cell = Cell::Text(TextCell::builder()
                              .content(content)
                              .formatting(vec![AnnotationWithOffset::new(mention_start as u32, Annotation::Mention(Mention::builder()
                                                                                                                       .name(name)
                                                                                                                       .user_id(user_id)
                                  .build()))])
        .build());
    let cell = notebook_cells_append(&client, notebook_id, None, None, vec![cell])
        .await
        .with_context(|| "Error appending cell to notebook")?
        .pop()
        .ok_or_else(|| anyhow!("No cells returned"))?;
    match args.output {
        MessageOutput::Table => {
            info!("Created cell");
            output_details(GenericKeyValue::from_cell(cell))
        }
        MessageOutput::Json => output_json(&cell),
    }
}

/// This buffers text to be written to a notebook cell

struct NotebookUrlReplacer<'a>(&'a HashMap<String, CrawledNotebook>);

impl<'a> Replacer for NotebookUrlReplacer<'a> {
    fn replace_append(&mut self, caps: &regex::Captures<'_>, dst: &mut String) {
        let notebook_id = caps.get(1).unwrap().as_str();
        if let Some(notebook) = self.0.get(notebook_id) {
            dst.push_str("./");
            dst.push_str(&notebook.file_name);
        } else {
            dst.push_str(caps.get(0).unwrap().as_str());
        }
    }
}

#[derive(Clone)]
struct CrawledNotebook {
    title: String,
    file_name: String,
    file_path: PathBuf,
    crawl_index: usize,
}

async fn handle_crawl_command(args: CrawlArgs) -> Result<()> {
    let mut crawled_notebooks = HashMap::new();
    let mut notebook_titles: HashMap<String, usize> = HashMap::new();
    let mut notebooks_to_crawl = VecDeque::new();

    let client = api_client_configuration(args.config, args.base_url.clone()).await?;
    let starting_notebook_id =
        interactive::notebook_picker(&client, args.notebook_id, None).await?;

    fs::create_dir_all(&args.out_dir)
        .await
        .with_context(|| "Error creating output directory")?;

    notebooks_to_crawl.push_back(starting_notebook_id);
    let mut crawl_index = 0;
    while let Some(notebook_id) = notebooks_to_crawl.pop_front() {
        if crawled_notebooks.contains_key(&notebook_id) {
            continue;
        }
        crawl_index += 1;
        let notebook = match notebook_get(&client, notebook_id).await {
            Ok(notebook) => notebook,
            Err(err) => {
                // TODO differentiate between 404 and other errors
                warn!("Error getting notebook {}: {}", notebook_id, err);
                continue;
            }
        };
        let notebook = serde_json::to_string(&notebook)?;
        let mut notebook: notebooks::Notebook = serde_json::from_str(&notebook)?;

        for cell in &mut notebook.cells {
            if let Some(formatting) = cell.formatting_mut() {
                for annotation in formatting {
                    if let formatting::Annotation::StartLink { url } = &mut annotation.annotation {
                        if url.starts_with(args.base_url.as_str()) {
                            if let Some(captures) = NOTEBOOK_ID_REGEX.captures(url) {
                                if let Some(notebook_id) = captures.get(1) {
                                    notebooks_to_crawl
                                        .push_back(Base64Uuid::from_str(notebook_id.as_str())?);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Ensure that multiple notebooks with the same title don't overwrite one another
        let number_suffix = if let Some(number) = notebook_titles.get(&notebook.title) {
            format!("_{number}")
        } else {
            notebook_titles.insert(notebook.title.clone(), 1);
            String::new()
        };

        let file_name = format!(
            "{}{}.md",
            notebook
                .title
                .replace(' ', "_")
                .replace('/', r"\/")
                .replace('\\', r"\\")
                .to_lowercase(),
            number_suffix
        );
        let file_path = args.out_dir.join(&file_name).with_extension("md");
        info!(
            "Writing notebook \"{}\" (ID: {}) to {}",
            notebook.title,
            notebook.id,
            file_path.display()
        );
        crawled_notebooks.insert(
            notebook_id,
            CrawledNotebook {
                title: notebook.title.clone(),
                file_name,
                file_path: file_path.clone(),
                crawl_index,
            },
        );
        let markdown = notebook_to_markdown(notebook);
        fs::write(file_path, markdown.as_bytes())
            .await
            .with_context(|| "Error saving markdown file")?;
    }

    // Convert the notebook URLs to relative markdown links
    for notebook in crawled_notebooks.values() {
        info!("Replacing notebook URLs in {}", notebook.file_name);
        let markdown = fs::read_to_string(&notebook.file_path)
            .await
            .with_context(|| "Error reading markdown file")?;

        let markdown = NOTEBOOK_URL_REGEX.replace_all(
            &markdown,
            NotebookUrlReplacer(
                &crawled_notebooks
                    .iter()
                    .map(|(id, notebook)| (id.to_string(), notebook.clone()))
                    .collect(),
            ),
        );
        fs::write(&notebook.file_path, markdown.as_bytes())
            .await
            .with_context(|| "Error replacing markdown file")?;
    }

    // Generate the SUMMARY.md file used by mdBook
    // https://rust-lang.github.io/mdBook/format/summary.html
    let mut notebooks = crawled_notebooks.values().collect::<Vec<_>>();
    notebooks.sort_by_key(|notebook| notebook.crawl_index);
    let mut summary = String::new();
    for notebook in notebooks {
        writeln!(
            &mut summary,
            "- [{}](./{})",
            notebook.title, notebook.file_name
        )?;
    }
    fs::write(args.out_dir.join("SUMMARY.md"), summary)
        .await
        .with_context(|| "Error writing SUMMARY.md")?;

    Ok(())
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Cache {
    pub user_id: Option<String>,
    pub user_name: Option<String>,
}

impl Cache {
    async fn load() -> Result<Self> {
        let path = cache_file_path();
        match fs::read_to_string(&path).await {
            Ok(string) => {
                let cache = toml::from_str(&string).with_context(|| "Error parsing cache file")?;
                debug!("Loaded cache from file: {:?}", path.display());
                Ok(cache)
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                debug!("No cache file found");
                Ok(Cache::default())
            }
            Err(err) => Err(err.into()),
        }
    }

    async fn save(&self) -> Result<()> {
        let string = toml::to_string_pretty(&self)?;
        let path = cache_file_path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .await
                .with_context(|| format!("Error creating cache directory: {:?}", dir.display()))?;
        }
        fs::write(&path, string)
            .await
            .with_context(|| format!("Error saving cache to file: {:?}", path.display()))?;
        debug!("saved config to: {}", path.display());
        Ok(())
    }
}

fn cache_file_path() -> PathBuf {
    ProjectDirs::from("com", "Fiberplane", "fiberplane-cli")
        .unwrap()
        .cache_dir()
        .join("cache.toml")
}

async fn handle_prometheus_redirect_command(args: PrometheusGraphToNotebookArgs) -> Result<()> {
    let client = Arc::new(api_client_configuration(args.config, args.base_url).await?);
    let workspace_id = interactive::workspace_picker(&client, args.workspace_id).await?;
    let notebook_id =
        interactive::notebook_picker(&client, args.notebook_id, Some(workspace_id)).await?;
    let notebook_url = NotebookUrlBuilder::new(workspace_id, notebook_id)
        .base_url(client.server.clone())
        .url()
        .expect("Error building URL");

    let listen_addr = (args.listen_host, args.port).into();
    let make_service = make_service_fn(move |_| {
        let client = client.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let client = client.clone();
                async move {
                    if !req.uri().path().starts_with("/graph") {
                        return Ok::<_, Error>(
                            Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(Body::from(
                                    "Prometheus-to-notebook can only be used for graph URLs",
                                ))
                                .expect("Error creating response"),
                        );
                    }
                    let query = req
                        .uri()
                        .query()
                        .and_then(|query| QString::from(query).get("g0.expr").map(String::from));

                    match query {
                        Some(query) => {
                            // Append cell to notebook and return the URL
                            let id = Base64Uuid::new().to_string();
                            if let Err(err) = notebook_cells_append(
                                &client,
                                notebook_id,
                                None,
                                None,
                                vec![Cell::Provider(ProviderCell::builder()
                                                        .id(id.clone())
                                                        .intent("prometheus,timeseries")
                                                        .query_data(format!(
                                        "application/x-www-form-urlencoded,query={query}"
                                    ))
                                                        .title("")
                                    .build())],
                            )
                            .await
                            {
                                error!("Error appending cell to notebook: {:?}", err);
                                return Ok::<_, Error>(
                                    Response::builder()
                                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                                        .body(Body::from("Error appending cell to notebook"))
                                        .unwrap(),
                                );
                            };

                            let url = NotebookUrlBuilder::new(workspace_id, notebook_id)
                                .base_url(client.server.clone())
                                .cell_id(id)
                                .url()
                                .expect("Error building URL");

                            debug!("Redirecting to: {}", url.as_str());

                            Ok::<_, Error>(
                                Response::builder()
                                    .status(StatusCode::TEMPORARY_REDIRECT)
                                    .header("Location", url.as_str())
                                    .body(Body::empty())
                                    .unwrap(),
                            )
                        }
                        None => Ok::<_, Error>(
                            Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(Body::from("Expected `g0.expr` query string parameter"))
                                .unwrap(),
                        ),
                    }
                }
            }))
        }
    });
    let server = Server::bind(&listen_addr).serve(make_service);

    info!(
        "Opening Prometheus graph URLs that start with: http://{listen_addr}/graph will now add them to the notebook: {notebook_url} ",
    );

    server.await?;

    Ok(())
}
