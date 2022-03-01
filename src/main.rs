use anyhow::{Context, Result};
use clap::{AppSettings, IntoApp, Parser};
use clap_complete::{generate, Shell};
use directories::ProjectDirs;
use manifest::Manifest;
use once_cell::sync::Lazy;
use std::fs::OpenOptions;
use std::io;
use std::io::{stdout, Write};
use std::process;
use std::time::{Duration, SystemTime};
use tokio::time::timeout;
use tracing::{trace, warn};
use update::retrieve_latest_version;

mod auth;
mod config;
mod manifest;
mod notebooks;
mod providers;
mod proxies;
mod templates;
mod triggers;
mod update;
mod version;

/// The current build manifest associated with this binary
pub static MANIFEST: Lazy<Manifest> = Lazy::new(Manifest::from_env);

/// The time before the fp command will try to do a version check again, in
/// seconds.
const VERSION_CHECK_DURATION: u64 = 60 * 60 * 24; // 24 hours

#[derive(Parser)]
#[clap(author, about, version, setting = AppSettings::PropagateVersion)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,

    /// Base URL for requests to Fiberplane
    #[clap(
        long,
        default_value = "https://fiberplane.com",
        env = "API_BASE",
        global = true
    )]
    // TODO parse as a URL
    base_url: String,

    /// Path to Fiberplane config.toml file
    #[clap(long, global = true)]
    // TODO parse this as a PathBuf
    config: Option<String>,

    /// Disables the version check
    #[clap(long, global = true, env)]
    disable_version_check: bool,
}

#[derive(Parser)]
enum SubCommand {
    /// Generate fp shell completions for your shell and print to stdout
    Completions {
        #[clap(arg_enum)]
        shell: Shell,
    },

    /// Login to Fiberplane and authorize the CLI to access your account
    #[clap()]
    Login,

    /// Logout from Fiberplane
    #[clap()]
    Logout,

    /// Commands related to Fiberplane Notebooks
    #[clap(aliases = &["notebook", "n"])]
    Notebooks(notebooks::Arguments),

    /// Interact with Fiberplane Providers
    #[clap()]
    Providers(providers::Arguments),

    /// Commands related to Fiberplane Proxies
    #[clap(alias = "proxy")]
    Proxies(proxies::Arguments),

    /// Commands related to Fiberplane Templates
    #[clap(alias = "template")]
    Templates(templates::Arguments),

    /// Interact with Fiberplane Triggers
    #[clap(alias = "trigger")]
    Triggers(triggers::Arguments),

    /// Update the current binary
    #[clap()]
    Update(update::Arguments),

    /// Display extra version information
    #[clap(aliases = &["v"])]
    Version(version::Arguments),
}

#[tokio::main]
async fn main() {
    initialize_logger();

    // We would like to override the builtin version display behavior, so we
    // will try to parse the arguments. If it failed, we will check if it was
    // the DisplayVersion error and show our version, otherwise just fallback to
    // clap's handling.
    let args = {
        match Arguments::try_parse() {
            Ok(arguments) => arguments,
            Err(err) => match err.kind {
                clap::ErrorKind::DisplayVersion => {
                    use std::io::Write;
                    version::output_version().await;

                    let _ = std::io::stdout().lock().flush();
                    let _ = std::io::stderr().lock().flush();

                    process::exit(0);
                }
                _ => {
                    err.exit();
                }
            },
        }
    };

    // Start the background version check, but skip it when running the `Update`
    // or `Version` command, or if the disable_version_check is set to true.
    let disable_version_check = args.disable_version_check
        || matches!(
            args.sub_command,
            Update(_) | Version(_) | Completions { .. }
        );

    let version_check_result = if disable_version_check {
        tokio::spawn(async { None })
    } else {
        tokio::spawn(async {
            match background_version_check().await {
                Ok(result) => result,
                Err(err) => {
                    trace!(%err, "version check failed");
                    None
                }
            }
        })
    };

    use SubCommand::*;
    let result = match args.sub_command {
        Login => auth::handle_login_command(args).await,
        Logout => auth::handle_logout_command(args).await,
        Notebooks(args) => notebooks::handle_command(args).await,
        Providers(args) => providers::handle_command(args).await,
        Proxies(args) => proxies::handle_command(args).await,
        Templates(args) => templates::handle_command(args).await,
        Triggers(args) => triggers::handle_command(args).await,
        Update(args) => update::handle_command(args).await,
        Version(args) => version::handle_command(args).await,
        Completions { shell } => {
            let mut app = Arguments::into_app();
            let app_name = app.get_name().to_string();
            let mut output = Vec::new();
            generate(shell, &mut app, app_name, &mut output);
            let output = String::from_utf8(output).unwrap();
            // There is some bug in the output generated by clap_complete that causes
            // the error "_arguments:comparguments:325: can only be called from completion function"
            // This solution fixes the problem: https://github.com/clap-rs/clap/issues/2488#issuecomment-999227749
            let output = if shell == Shell::Zsh {
                let mut lines = output.lines();
                // Remove the first and last lines
                lines.next();
                lines.next_back();
                let mut modified = String::with_capacity(output.len());
                modified.push_str("#compdef _fp fp\n");
                for line in lines {
                    modified.push_str(line);
                    modified.push('\n');
                }
                modified
            } else {
                output
            };
            stdout().lock().write_all(output.as_bytes()).unwrap();

            Ok(())
        }
    };

    if let Err(ref err) = result {
        eprintln!("Command did not finish successfully: {:?}", err);
    }

    // Wait for an extra second for the background check to finish
    if let Ok(version_check_result) = timeout(Duration::from_secs(1), version_check_result).await {
        match version_check_result {
            Ok(Some(new_version)) => {
                eprintln!("A new version of fp is available (version: {}). Use `fp update` to update your current fp binary", new_version);
            }
            Ok(None) => trace!("background version check skipped or no new version available"),
            Err(err) => warn!(%err, "background version check failed"),
        }
    }

    if result.is_err() {
        use std::io::Write;

        let _ = std::io::stdout().lock().flush();
        let _ = std::io::stderr().lock().flush();

        process::exit(1);
    }
}

fn initialize_logger() {
    // Initialize the builder with some defaults
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(io::stderr)
        .try_init()
        .expect("unable to initialize logging");
}

/// Fetches the latest remote version for fp and determines whether a new
/// version is available. It will only check once per 24 hours.
pub async fn background_version_check() -> Result<Option<String>> {
    let config_dir = ProjectDirs::from("com", "Fiberplane", "fiberplane-cli")
        .unwrap()
        .config_dir()
        .to_owned();
    let check_file = config_dir.join("version_check");

    let should_check = match std::fs::metadata(&check_file) {
        Ok(metadata) => {
            let date = metadata
                .modified()
                .context("failed to check the modified date on the version check file")?;
            date < (SystemTime::now() - Duration::from_secs(VERSION_CHECK_DURATION))
        }
        Err(err) => {
            // This will most likely be caused by the file not existing, so we
            // will just trace it and go ahead with the version check.
            trace!(%err, "checking the update file check resulted in a error");
            true
        }
    };

    // We've checked the version recently, so just return early indicating that
    // no update should be done.
    if !should_check {
        return Ok(None);
    }

    let remote_version = retrieve_latest_version()
        .await
        .context("failed to check for remote version")?;

    // Ensure that the config directory exists
    if let Err(err) = std::fs::create_dir_all(&config_dir) {
        trace!(%err, "unable to create the config dir");
    } else {
        // Create a new file or truncate the existing one. Both should result in a
        // new modified date (this is like `touch` but it will truncate any existing
        // files).
        if let Err(err) = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&check_file)
        {
            trace!(%err, "unable to create the version check file");
        };
    };

    if remote_version != MANIFEST.build_version {
        Ok(Some(remote_version))
    } else {
        Ok(None)
    }
}
