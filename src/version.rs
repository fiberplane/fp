use crate::manifest::retrieve_manifest;
use crate::MANIFEST;
use anyhow::{Context, Result};
use clap::{ArgEnum, Parser};
use directories::ProjectDirs;
use std::fs::OpenOptions;
use std::time::SystemTime;
use tracing::{error, trace};

#[derive(Parser)]
pub struct Arguments {
    /// output type to use
    #[clap(long, short, default_value = "display", arg_enum)]
    pub output: OutputType,

    #[clap(from_global)]
    pub disable_version_check: bool,
}

#[derive(ArgEnum, Clone)]
pub enum OutputType {
    /// Display as a human readable list
    Display,

    /// Display as a JSON encoded object
    Json,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    let result = match args.output {
        OutputType::Display => output_display(&args).await,
        OutputType::Json => output_json(&args).await,
    };

    // Force a version check every time this command gets run, unless
    // the --disable-version-check flag is set.
    if !args.disable_version_check {
        match retrieve_manifest(&MANIFEST.rustc_host_triple).await {
            Ok(remote_manifest) => {
                if remote_manifest.build_version == *MANIFEST.build_version {
                    eprintln!("You are running the latest version of fp");
                } else {
                    eprint!("A new version of fp is available (version: {}). Use `fp update` to update your current fp binary", remote_manifest.build_version);
                }
            }
            Err(err) => error!(%err, "unable to retrieve manifest"),
        }
    }

    result
}

async fn output_display(_args: &Arguments) -> Result<()> {
    eprintln!("Build Timestamp: {}", MANIFEST.build_timestamp);
    eprintln!("Build Version: {}", MANIFEST.build_version);
    eprintln!("Commit Date: {}", MANIFEST.commit_date);
    eprintln!("Commit SHA: {}", MANIFEST.commit_sha);
    eprintln!("Commit Branch: {}", MANIFEST.commit_branch);
    eprintln!("rustc Version: {}", MANIFEST.rustc_version);
    eprintln!("rustc Channel: {}", MANIFEST.rustc_channel);
    eprintln!("rustc Host Triple {}", MANIFEST.rustc_host_triple);
    eprintln!("rustc Commit SHA {}", MANIFEST.rustc_commit_sha);
    eprintln!("cargo Target Triple {}", MANIFEST.cargo_target_triple);
    eprintln!("cargo Profile: {}", MANIFEST.cargo_profile);

    Ok(())
}

async fn output_json(_args: &Arguments) -> Result<()> {
    serde_json::to_writer(std::io::stdout(), &*MANIFEST)?;
    Ok(())
}
