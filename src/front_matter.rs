use crate::config::api_client_configuration;
use crate::interactive::{front_matter_collection_picker, workspace_picker};
use anyhow::{Context, Result};
use clap::{Parser, ValueHint};
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::front_matter_schemas::FrontMatterSchema;
use fiberplane::models::names::Name;
use fiberplane::models::workspaces::NewWorkspaceFrontMatterSchema;
use std::{io::stdin, path::PathBuf};
use url::Url;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
pub enum SubCommand {
    /// Set an existing front matter collection with the given name to the given schema
    Set(SetArgs),

    /// Create a front matter collection with the given name and the given schema
    Create(CreateArgs),

    /// Get the front matter collection with the given name
    Get(GetArgs),

    /// Delete the front matter collection with the given name
    Delete(DeleteArgs),
}

#[derive(Parser)]
pub struct SetArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Name of the front matter collection to set.
    #[clap(short, long)]
    name: Option<Name>,

    /// Path to the json file containing the collection description.
    ///
    /// Use `-` to read from stdin
    #[clap(value_hint = ValueHint::FilePath)]
    json_path: PathBuf,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    #[clap(from_global)]
    token: Option<String>,
}

#[derive(Parser)]
pub struct CreateArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Name of the front matter collection to create.
    #[clap(short, long)]
    name: Name,

    /// Path to the json file containing the collection description.
    ///
    /// Use `-` to read from stdin
    #[clap(value_hint = ValueHint::FilePath)]
    json_path: PathBuf,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    #[clap(from_global)]
    token: Option<String>,
}

#[derive(Parser)]
pub struct GetArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Name of the front matter collection to set.
    #[clap(short, long)]
    name: Option<Name>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    #[clap(from_global)]
    token: Option<String>,
}

#[derive(Parser)]
pub struct DeleteArgs {
    /// Workspace to use
    #[clap(from_global)]
    workspace_id: Option<Base64Uuid>,

    /// Name of the front matter collection to set.
    #[clap(short, long)]
    name: Option<Name>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,

    #[clap(from_global)]
    token: Option<String>,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Set(args) => handle_set_command(args).await,
        SubCommand::Create(args) => handle_create_command(args).await,
        SubCommand::Get(args) => handle_get_command(args).await,
        SubCommand::Delete(args) => handle_delete_command(args).await,
    }
}

pub async fn handle_set_command(args: SetArgs) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url.clone()).await?;

    let (workspace_id, fmc_name) =
        front_matter_collection_picker(&client, args.workspace_id, args.name).await?;

    let front_matter_schema: FrontMatterSchema = if args.json_path.to_str().unwrap() == "-" {
        let content: String = stdin()
            .lines()
            .collect::<Result<_, _>>()
            .with_context(|| "cannot read content from stdin")?;
        serde_json::from_str(&content).with_context(|| "cannot parse content as schema")?
    } else {
        let content: String = std::fs::read_to_string(&args.json_path)
            .with_context(|| "cannot open source file for collection")?;
        serde_json::from_str(&content).with_context(|| "cannot parse content as schema")?
    };

    client
        .workspace_front_matter_schema_create(
            workspace_id,
            NewWorkspaceFrontMatterSchema::builder()
                .name(fmc_name.to_string())
                .schema(front_matter_schema)
                .build(),
        )
        .await?;

    Ok(())
}

pub async fn handle_create_command(args: CreateArgs) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url.clone()).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;

    let front_matter_schema: FrontMatterSchema = if args.json_path.to_str().unwrap() == "-" {
        let content: String = stdin()
            .lines()
            .collect::<Result<_, _>>()
            .with_context(|| "cannot read content from stdin")?;
        serde_json::from_str(&content).with_context(|| "cannot parse content as schema")?
    } else {
        let content: String = std::fs::read_to_string(&args.json_path)
            .with_context(|| "cannot open source file for collection")?;
        serde_json::from_str(&content).with_context(|| "cannot parse content as schema")?
    };

    client
        .workspace_front_matter_schema_create(
            workspace_id,
            NewWorkspaceFrontMatterSchema::builder()
                .name(args.name.to_string())
                .schema(front_matter_schema)
                .build(),
        )
        .await?;

    Ok(())
}

pub async fn handle_get_command(args: GetArgs) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url.clone()).await?;

    let (workspace_id, fmc_name) =
        front_matter_collection_picker(&client, args.workspace_id, args.name).await?;

    let fmc = client
        .workspace_front_matter_schema_get_by_name(workspace_id, &fmc_name)
        .await?;

    println!(
        "{}",
        serde_json::to_string(&fmc).expect("Front Matter Collections are JSON-serializable")
    );

    Ok(())
}

pub async fn handle_delete_command(args: DeleteArgs) -> Result<()> {
    let client = api_client_configuration(args.token, args.config, args.base_url.clone()).await?;

    let (_workspace_id, _fmc_name) =
        front_matter_collection_picker(&client, args.workspace_id, args.name).await?;

    unimplemented!(
        "The workspace_front_matter_schema_delete endpoint is missing from the API for now."
    );
    // workspace_front_matter_schema_delete(&client, workspace_id, &fmc_name).await?;

    // Ok(())
}
