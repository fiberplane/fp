use crate::config::api_client_configuration;
use crate::output::{output_details, output_json, GenericKeyValue};
use crate::templates::NOTEBOOK_ID_REGEX;
use anyhow::{anyhow, Context, Error, Result};
use bytes::Bytes;
use clap::{ArgEnum, Parser};
use directories::ProjectDirs;
use fiberplane::protocols::{core, formatting};
use fiberplane_markdown::notebook_to_markdown;
use fp_api_client::apis::configuration::Configuration;
use fp_api_client::apis::default_api::{
    get_notebook, get_profile, notebook_cell_append_text, notebook_cells_append,
};
use fp_api_client::models::{Annotation, Cell, CellAppendText};
use futures::StreamExt;
use lazy_static::lazy_static;
use regex::{Regex, Replacer};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::{env::current_dir, io::ErrorKind, path::PathBuf, process::Stdio, str::FromStr};
use std::{fmt::Write, time::Duration};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::io::{self, AsyncWriteExt};
use tokio::{fs, process::Command, time::interval};
use tokio_util::io::ReaderStream;
use tracing::{debug, info, warn};
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

    /// Execute a shell command and pipe the output to a notebook
    Exec(ExecArgs),

    /// Starting with the given notebook, recursively crawl all linked notebooks
    /// and save them to the given directory as Markdown
    Crawl(CrawlArgs),
}

#[derive(Parser)]
struct MessageArgs {
    /// The notebook to append the message to
    #[clap(long, short, env)]
    notebook_id: String,

    /// The message to append
    message: Vec<String>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    /// Output type to display
    #[clap(long, short, default_value = "table", arg_enum)]
    output: MessageOutput,
}

#[derive(Parser, Clone)]
struct ExecArgs {
    /// The notebook to append the message to
    #[clap(long, short, env)]
    notebook_id: String,

    /// The command to run
    command: String,

    /// Args to pass to the command
    args: Vec<String>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    /// Output type to display
    #[clap(long, short, default_value = "command", arg_enum)]
    output: ExecOutput,
}

#[derive(Parser)]
struct CrawlArgs {
    notebook: String,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    #[clap(long, default_value = "10")]
    concurrent_downloads: u8,

    #[clap(long, short)]
    out_dir: PathBuf,
}

#[derive(ArgEnum, Clone)]
enum MessageOutput {
    /// Output the result as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

#[derive(ArgEnum, Clone, PartialEq)]
enum ExecOutput {
    /// Output the result of the command
    Command,

    /// Output the cell details as a table
    Table,

    /// Output the cell details as a JSON encoded object
    Json,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Message(args) => handle_message_command(args).await,
        SubCommand::Exec(args) => handle_exec_command(args).await,
        SubCommand::Crawl(args) => handle_crawl_command(args).await,
    }
}

async fn handle_message_command(args: MessageArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let mut cache = Cache::load().await?;

    // If we don't already know the user name, load it from the API and save it
    let (user_id, name) = match (cache.user_id, cache.user_name) {
        (Some(user_id), Some(user_name)) => (user_id, user_name),
        _ => {
            let user = get_profile(&config)
                .await
                .with_context(|| "Error getting user profile")?;
            cache.user_name = Some(user.name.clone());
            cache.user_id = Some(user.id.clone());
            cache.save().await?;
            (user.id, user.name)
        }
    };

    let timestamp_prefix = format!("💬 {} ", OffsetDateTime::now_utc().format(&Rfc3339)?);
    // Note we don't use .len() because it returns the byte length as opposed to the char length (which is different because of the emoji)
    let mention_start = timestamp_prefix.chars().count();
    let prefix = format!("{}@{}:  ", timestamp_prefix, name);
    let content = format!("{}{}", prefix, args.message.join(" "));

    let cell = Cell::TextCell {
        id: String::new(),
        content,
        formatting: Some(vec![Annotation::MentionAnnotation {
            name,
            user_id,
            offset: mention_start as i32,
        }]),
        read_only: None,
    };
    let cell = notebook_cells_append(&config, &args.notebook_id, vec![cell])
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
struct CellWriter {
    args: ExecArgs,
    config: Configuration,
    cell: Option<core::Cell>,
    buffer: Vec<Bytes>,
}

impl CellWriter {
    pub fn new(args: ExecArgs, config: Configuration) -> Self {
        Self {
            args,
            config,
            cell: None,
            buffer: Vec::new(),
        }
    }

    pub fn append(&mut self, data: Bytes) {
        self.buffer.push(data);
    }

    pub async fn write_to_cell(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let mut output = String::new();
        let buffer = self.buffer.split_off(0);
        for chunk in buffer {
            output.push_str(&String::from_utf8_lossy(&chunk));
        }

        // Either create a new cell or append to the existing one
        match &mut self.cell {
            None => {
                let timestamp = OffsetDateTime::now_utc().format(&Rfc3339)?;
                let cwd = current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                let content = format!(
                    "{}\n{} ❯ {} {}\n{}",
                    timestamp,
                    cwd,
                    self.args.command,
                    self.args.args.join(" "),
                    output
                );
                let cell = Cell::CodeCell {
                    id: String::new(),
                    content,
                    syntax: None,
                    read_only: None,
                };

                let cell = notebook_cells_append(&self.config, &self.args.notebook_id, vec![cell])
                    .await
                    .with_context(|| "Error appending cell to notebook")?
                    .pop()
                    .ok_or_else(|| anyhow!("No cells returned"))?;
                self.cell = Some(serde_json::from_value(serde_json::to_value(cell)?)?);
            }
            Some(cell) => {
                notebook_cell_append_text(
                    &self.config,
                    &self.args.notebook_id,
                    cell.id(),
                    CellAppendText {
                        content: output,
                        formatting: None,
                    },
                )
                .await
                .with_context(|| format!("Error appending text to cell {}", cell.id()))?;
            }
        }
        Ok::<_, Error>(())
    }

    pub fn into_output_cell(self) -> Option<core::Cell> {
        self.cell
    }
}

async fn handle_exec_command(args: ExecArgs) -> Result<()> {
    debug!("Running command: \"{}\"", args.command);
    let config = api_client_configuration(args.config.clone(), &args.base_url).await?;
    let mut child = Command::new(&args.command)
        .args(&args.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::inherit())
        .spawn()
        .with_context(|| "Error spawning child process to run command")?;

    let mut child_stdout = ReaderStream::new(child.stdout.take().unwrap());
    let mut child_stderr = ReaderStream::new(child.stderr.take().unwrap());
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();

    // Spawn a task to run the command.
    // Send the output to the channel so we can pipe it to a notebook cell.
    // If the output format is Command, also pipe the output to stdout/stderr
    let mut send_interval = interval(Duration::from_millis(250));
    let mut cell_writer = CellWriter::new(args.clone(), config);
    loop {
        tokio::select! {
            biased;
            _ = child.wait() => {
                cell_writer.write_to_cell().await?;
                break;
            }
            _ = send_interval.tick() => {
                cell_writer.write_to_cell().await?;
            }
            chunk = child_stdout.next() => {
                if let Some(Ok(chunk)) = chunk {
                    if args.output == ExecOutput::Command {
                        stdout.write_all(&chunk).await?;
                    }
                    cell_writer.append(chunk);
                }
            }
            chunk = child_stderr.next() => {
                if let Some(Ok(chunk)) = chunk {
                    if args.output == ExecOutput::Command {
                        stderr.write_all(&chunk).await?;
                    }
                    cell_writer.append(chunk);
                }
            }
        }
    }

    let output_cell = cell_writer.into_output_cell();
    let mut url = args
        .base_url
        .join("/notebook/")
        .unwrap()
        .join(&args.notebook_id)
        .unwrap();
    if let Some(cell) = &output_cell {
        url.set_fragment(Some(cell.id()));
    };

    let cell: Cell = serde_json::from_value(serde_json::to_value(output_cell.unwrap())?)?;
    match args.output {
        ExecOutput::Command => {
            info!("\n   --> Created cell: {}", url);
            Ok(())
        }
        ExecOutput::Table => {
            info!("Created cell");
            output_details(GenericKeyValue::from_cell(cell))
        }
        ExecOutput::Json => output_json(&cell),
    }
}

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
    let starting_notebook_id = NOTEBOOK_ID_REGEX
        .captures(&args.notebook)
        .and_then(|c| c.get(1))
        .ok_or_else(|| anyhow!("Invalid notebook URL or ID"))?
        .as_str();

    let config = api_client_configuration(args.config, &args.base_url).await?;

    fs::create_dir_all(&args.out_dir)
        .await
        .with_context(|| "Error creating output directory")?;

    notebooks_to_crawl.push_back(starting_notebook_id.to_string());
    let mut crawl_index = 0;
    while let Some(notebook_id) = notebooks_to_crawl.pop_front() {
        if crawled_notebooks.contains_key(&notebook_id) {
            continue;
        }
        crawl_index += 1;
        let notebook = match get_notebook(&config, &notebook_id).await {
            Ok(notebook) => notebook,
            Err(err) => {
                // TODO differentiate between 404 and other errors
                warn!("Error getting notebook {}: {}", notebook_id, err);
                continue;
            }
        };
        let notebook = serde_json::to_string(&notebook)?;
        let mut notebook: core::Notebook = serde_json::from_str(&notebook)?;

        for cell in &mut notebook.cells {
            if let Some(formatting) = cell.formatting_mut() {
                for annotation in formatting {
                    if let formatting::Annotation::StartLink { url } = &mut annotation.annotation {
                        if url.starts_with(args.base_url.as_str()) {
                            if let Some(captures) = NOTEBOOK_ID_REGEX.captures(url) {
                                if let Some(notebook_id) = captures.get(1) {
                                    notebooks_to_crawl.push_back(notebook_id.as_str().to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Ensure that multiple notebooks with the same title don't overwrite one another
        let number_suffix = if let Some(number) = notebook_titles.get(&notebook.title) {
            format!("_{}", number)
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
            notebook_id.clone(),
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
        let markdown =
            NOTEBOOK_URL_REGEX.replace_all(&markdown, NotebookUrlReplacer(&crawled_notebooks));
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
