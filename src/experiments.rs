use crate::config::api_client_configuration;
use crate::output::{output_details, output_json, GenericKeyValue};
use anyhow::{anyhow, Context, Result};
use clap::{ArgEnum, Parser};
use directories::ProjectDirs;
use fp_api_client::apis::default_api::{get_profile, notebook_cells_append};
use fp_api_client::models::{Annotation, Cell};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::{env::current_dir, io::ErrorKind, path::PathBuf, process::Stdio};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::io::{self, AsyncWriteExt};
use tokio::{fs, process::Command};
use tokio_util::io::ReaderStream;
use tracing::{debug, info};
use url::Url;

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

#[derive(Parser)]
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
    #[clap(long, short, default_value = "table", arg_enum)]
    output: MessageOutput,
}

#[derive(ArgEnum, Clone)]
enum MessageOutput {
    /// Output the result as a table
    Table,

    /// Output the result as a JSON encoded object
    Json,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Message(args) => handle_message_command(args).await,
        SubCommand::Exec(args) => handle_exec_command(args).await,
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
    let cell = notebook_cells_append(&config, &args.notebook_id, Some(vec![cell]))
        .await
        .with_context(|| "Error appending cell to notebook")?
        .pop()
        .ok_or_else(|| anyhow!("No cells returned"))?;
    info!("Created cell");
    match args.output {
        MessageOutput::Table => output_details(GenericKeyValue::from_cell(cell)),
        MessageOutput::Json => output_json(&cell),
    }
}

async fn handle_exec_command(args: ExecArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let mut child = Command::new(&args.command)
        .args(&args.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::inherit())
        .spawn()
        .with_context(|| "Error spawning child process to run command")?;

    // Pipe stdout and stderr to the parent process AND merge them both
    // into a single output buffer that we'll send to the notebook
    let mut child_stdout = ReaderStream::new(child.stdout.take().unwrap());
    let mut child_stderr = ReaderStream::new(child.stderr.take().unwrap());
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    let mut output: Vec<u8> = Vec::new();
    loop {
        tokio::select! {
            biased;
            _ = child.wait() => {
                break;
            }
            chunk = child_stdout.next() => {
                if let Some(Ok(chunk)) = chunk {
                    output.extend(&chunk);
                    stdout.write_all(&chunk).await?;
                }
            }
            chunk = child_stderr.next() => {
                if let Some(Ok(chunk)) = chunk {
                    output.extend(&chunk);
                    stderr.write_all(&chunk).await?;
                }
            }
        }
    }

    let content = format!(
        "{timestamp}\n{cwd} ❯ {command} {args}\n{output}",
        timestamp = OffsetDateTime::now_utc().format(&Rfc3339)?,
        cwd = current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        command = args.command,
        args = args.args.join(" "),
        output = String::from_utf8(output)
            .with_context(|| "Command output was not valid UTF-8")?
            .trim_end()
    );

    let cell = Cell::CodeCell {
        id: String::new(),
        content,
        syntax: None,
        read_only: None,
    };

    let cell = notebook_cells_append(&config, &args.notebook_id, Some(vec![cell]))
        .await
        .with_context(|| "Error appending cell to notebook")?
        .pop()
        .ok_or_else(|| anyhow!("No cells returned"))?;
    info!("Created cell");
    match args.output {
        MessageOutput::Table => output_details(GenericKeyValue::from_cell(cell)),
        MessageOutput::Json => output_json(&cell),
    }
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
