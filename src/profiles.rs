use crate::config::{default_profile_name, Config, FP_CONFIG_DIR};
use crate::interactive::{text_opt, text_req};
use anyhow::{anyhow, bail, Result};
use clap::Parser;
use tracing::info;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    /// Create a new profile
    Create(CreateArgs),

    /// List all profiles
    List,

    /// Delete a profile
    Delete(DeleteArgs),

    /// Set a profile to a default profile
    SetDefault(SetDefaultArgs),
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Create(args) => handle_profile_create(args).await,
        SubCommand::List => handle_profile_list().await,
        SubCommand::Delete(args) => handle_profile_delete(args).await,
        SubCommand::SetDefault(args) => handle_profile_set_default(args).await,
    }
}

#[derive(Parser)]
struct CreateArgs {
    /// Name of the new profile. Must be allowed as a file name depending on your file system restrictions
    #[clap(long)]
    name: Option<String>,

    /// Endpoint which this profile should contact
    #[clap(long)]
    endpoint: Option<String>,
}

async fn handle_profile_create(args: CreateArgs) -> Result<()> {
    let name = text_req("Name", args.name, None)?.to_lowercase();

    if name.contains(' ') || name.contains('.') {
        bail!("Name cannot contain spaces or dots");
    }

    let endpoint = text_opt(
        "Endpoint",
        args.endpoint,
        Some("https://studio.fiberplane.com".to_string()),
    );

    let config = Config {
        api_token: None,
        endpoint,
    };

    config.save(Some(&name)).await?;

    info!("Successfully created new profile. Login on that profile with `fp +{name} login`");
    Ok(())
}

async fn handle_profile_list() -> Result<()> {
    info!("List of profiles:");

    for entry in std::fs::read_dir(FP_CONFIG_DIR.as_path())? {
        let entry = entry?;
        let file_name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow!("failed to convert osstring to str"))?;

        if !file_name.ends_with(".toml") {
            continue;
        }

        info!("- {}", file_name.replace(".toml", ""));
    }

    Ok(())
}

#[derive(Parser)]
struct DeleteArgs {
    /// Name of the profile to delete
    #[clap(long)]
    name: Option<String>,
}

async fn handle_profile_delete(args: DeleteArgs) -> Result<()> {
    let name = text_req("Name", args.name, None)?.to_lowercase();

    if name == default_profile_name().await? {
        bail!("Cannot delete default profile. Please switch it first with `fp profile set-default`")
    }

    tokio::fs::remove_file(FP_CONFIG_DIR.join(format!("{name}.toml"))).await?;

    info!("Successfully deleted profile.");
    Ok(())
}

#[derive(Parser)]
struct SetDefaultArgs {
    /// Name of the profile to make default
    #[clap(long)]
    name: Option<String>,
}

async fn handle_profile_set_default(args: SetDefaultArgs) -> Result<()> {
    let name = text_req("Name", args.name, None)?.to_lowercase();

    if tokio::fs::metadata(FP_CONFIG_DIR.join(format!("{name}.toml")))
        .await
        .is_err()
    {
        bail!("Profile not found");
    }

    tokio::fs::write(FP_CONFIG_DIR.join("default_profile"), name).await?;

    info!("Successfully set default profile");
    Ok(())
}
