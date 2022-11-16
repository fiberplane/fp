use crate::config::Config;
use clap::Parser;
use std::path::PathBuf;
use tracing::{error, info};
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

pub(crate) async fn handle_command(args: Arguments) {
    let mut config = Config::load(args.config).await?;

    config.analytics = args.enabled;

    match config.save().await {
        Ok(_) => {
            if !args.silent {
                info!("Successfully saved analytics preference");
            }
        }
        Err(err) => {
            error!(
                "Error saving analytics preference to config file {}: {:?}",
                config.path.display(),
                err
            );
        }
    }
}
