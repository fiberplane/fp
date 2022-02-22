use crate::{retrieve_latest_version, MANIFEST};
use anyhow::Result;
use clap::{ArgEnum, Parser};
use std::io::Write;
use tracing::error;

#[derive(Parser)]
pub struct Arguments {
    /// output type to use
    #[clap(long, short, default_value = "version", arg_enum)]
    pub output: OutputType,

    #[clap(from_global)]
    pub disable_version_check: bool,
}

#[derive(ArgEnum, Clone)]
pub enum OutputType {
    /// Only display the version
    Version,

    /// Show all the build information
    Verbose,

    /// Show all the build information encoded as JSON
    Json,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    let result = match args.output {
        OutputType::Version => output_version(&args).await,
        OutputType::Verbose => output_verbose(&args).await,
        OutputType::Json => output_json(&args).await,
    };

    // Force a version check every time this command gets run, unless
    // the --disable-version-check flag is set.
    if !args.disable_version_check {
        match retrieve_latest_version().await {
            Ok(remote_version) => {
                if remote_version == *MANIFEST.build_version {
                    eprintln!("You are running the latest version of fp");
                } else {
                    eprintln!("A new version of fp is available (version: {}). Use `fp update` to update your current fp binary", remote_version);
                }
            }
            Err(err) => error!(%err, "unable to retrieve manifest"),
        }
    }

    result
}

async fn output_version(_args: &Arguments) -> Result<()> {
    println!("{}", MANIFEST.build_version);

    Ok(())
}

async fn output_verbose(_args: &Arguments) -> Result<()> {
    println!("Build Timestamp: {}", MANIFEST.build_timestamp);
    println!("Build Version: {}", MANIFEST.build_version);
    println!("Commit Date: {}", MANIFEST.commit_date);
    println!("Commit SHA: {}", MANIFEST.commit_sha);
    println!("Commit Branch: {}", MANIFEST.commit_branch);
    println!("rustc Version: {}", MANIFEST.rustc_version);
    println!("rustc Channel: {}", MANIFEST.rustc_channel);
    println!("rustc Host Triple {}", MANIFEST.rustc_host_triple);
    println!("rustc Commit SHA {}", MANIFEST.rustc_commit_sha);
    println!("cargo Target Triple {}", MANIFEST.cargo_target_triple);
    println!("cargo Profile: {}", MANIFEST.cargo_profile);

    Ok(())
}

async fn output_json(_args: &Arguments) -> Result<()> {
    serde_json::to_writer(std::io::stdout(), &*MANIFEST)?;
    writeln!(std::io::stdout())?;
    Ok(())
}
