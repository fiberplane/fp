use crate::config::Config;
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::info;
use url::Url;

#[derive(Parser)]
pub(crate) struct Arguments {
    #[clap()]
    enabled: bool,

    #[clap(long, default_value = "false")]
    silent: bool,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

pub(crate) async fn handle_command(args: Arguments) -> Result<()> {
    let mut config = Config::load(args.config).await?;

    config.analytics = args.enabled;
    config.save().await?;

    if !args.silent {
        info!("Successfully saved analytics preference");
    }

    Ok(())
}
