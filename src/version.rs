use crate::{
    output::{output_details, GenericKeyValue},
    update::retrieve_latest_version,
    MANIFEST,
};
use anyhow::Result;
use clap::{Parser, ValueEnum};
use tracing::{debug, error, info};

#[derive(Parser)]
pub struct Arguments {
    /// output type to use
    #[clap(long, short, default_value = "version", value_enum)]
    pub output: OutputType,

    #[clap(from_global)]
    pub disable_version_check: bool,
}

#[derive(ValueEnum, Clone)]
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
        OutputType::Version => {
            output_version().await;
            Ok(())
        }
        OutputType::Verbose => {
            output_verbose().await?;
            Ok(())
        }
        OutputType::Json => output_json().await,
    };

    // Force a version check every time this command gets run, unless
    // the --disable-version-check flag is set.
    if !args.disable_version_check {
        debug!("Starting version check");
        match retrieve_latest_version().await {
            Ok(remote_version) => {
                let version = &*MANIFEST.build_version;
                if remote_version == version {
                    info!(%version, "You are running the latest version of fp");
                } else {
                    info!("A new version of fp is available (version: {}). Use `fp update` to update your current fp binary", remote_version);
                }
            }
            Err(err) => error!(%err, "unable to retrieve manifest"),
        }
    }

    result
}

pub async fn output_version() {
    println!("{}", MANIFEST.build_version);
}

async fn output_verbose() -> Result<()> {
    let manifest = GenericKeyValue::from_manifest(MANIFEST.clone());

    output_details(manifest)
}

async fn output_json() -> Result<()> {
    crate::output::output_json(&*MANIFEST)
}
