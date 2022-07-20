use anyhow::{anyhow, Context, Error, Result};
use clap::{AppSettings, IntoApp, Parser};
use clap_complete::{generate, Shell};
use directories::ProjectDirs;
use manifest::Manifest;
use once_cell::sync::Lazy;
use std::fs::OpenOptions;
use std::io::{stdout, Write};
use std::path::PathBuf;
use std::process;
use std::str::FromStr;
use std::time::{Duration, SystemTime};
use std::{env, io};
use tokio::time::timeout;
use tracing::{error, info, trace, warn};
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt::format;
use update::retrieve_latest_version;
use url::Url;

mod auth;
mod config;
mod events;
mod experiments;
mod labels;
mod manifest;
mod notebooks;
mod output;
mod providers;
mod proxies;
mod run;
mod shell;
mod templates;
mod triggers;
mod update;
mod users;
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

    /// Base URL to the Fiberplane API
    #[clap(
        long,
        default_value = "https://fiberplane.com",
        env = "API_BASE",
        global = true,
        help_heading = "GLOBAL OPTIONS"
    )]
    base_url: Url,

    /// Path to Fiberplane config file
    #[clap(long, global = true, env, help_heading = "GLOBAL OPTIONS")]
    config: Option<PathBuf>,

    /// Disables the version check
    #[clap(long, global = true, env, help_heading = "GLOBAL OPTIONS")]
    disable_version_check: bool,

    /// Display verbose logs
    #[clap(short, long, global = true, env, help_heading = "GLOBAL OPTIONS")]
    verbose: bool,

    /// Path to log file
    #[clap(long, global = true, env)]
    log_file: Option<PathBuf>,
}

#[derive(Parser)]
enum SubCommand {
    /// Generate fp shell completions for your shell and print to stdout
    Completions {
        #[clap(arg_enum)]
        shell: clap_complete::Shell,
    },

    /// Experimental commands 🧪
    ///
    /// These commands are not stable and may change at any time.
    #[clap(aliases = &["experiment", "x"])]
    Experiments(experiments::Arguments),

    /// Login to Fiberplane and authorize the CLI to access your account
    #[clap()]
    Login,

    /// Logout from Fiberplane
    #[clap()]
    Logout,

    /// Interact with labels
    ///
    /// Labels allow you to organize your notebooks.
    #[clap(alias = "label")]
    Labels(labels::Arguments),

    /// Interact with notebooks
    ///
    /// Notebooks are the main resource that Studio exposes.
    #[clap(alias = "notebook")]
    Notebooks(notebooks::Arguments),

    /// Interact with providers
    ///
    /// Providers are wasm files that contain the logic to retrieve data based
    /// on a query. This is being used by Studio and Proxy.
    #[clap(alias = "provider")]
    Providers(providers::Arguments),

    /// Interact with Fiberplane proxies
    ///
    /// The Fiberplane proxy allows you to expose services that are hosted
    /// within your network without exposing them or sharing credentials.
    #[clap(alias = "proxy")]
    Proxies(proxies::Arguments),

    /// Run a command and send the output to a notebook
    ///
    /// Note: to run a command with pipes, you must wrap the command in quotes
    /// (otherwise, your shell will think you want to pipe the output of `fp run`).
    /// For example, `fp run "echo hello world | grep hello"`
    #[clap(trailing_var_arg = true)]
    Run(run::Arguments),

    /// Interact with templates
    ///
    /// Templates allow you to create notebooks based on jsonnet.
    #[clap(alias = "template")]
    Templates(templates::Arguments),

    /// Interact with triggers
    ///
    /// Triggers allow you to expose webhooks that will expand templates.
    /// This could be used for alertmanager, for example.
    #[clap(alias = "trigger")]
    Triggers(triggers::Arguments),

    /// Launch a recorded shell session that'll show up in the notebook
    #[clap()]
    Shell(shell::Arguments),
    /// Interact with events
    ///
    /// Events allow you to mark a specific point in time when something occurred, such as a deployment.
    #[clap(alias = "event")]
    Events(events::Arguments),

    /// Update the current FP binary
    #[clap()]
    Update(update::Arguments),

    /// Interact with user details
    #[clap(alias = "user")]
    Users(users::Arguments),

    /// Display extra version information
    #[clap()]
    Version(version::Arguments),
}

#[tokio::main]
async fn main() {
    // We would like to override the builtin version display behavior, so we
    // will try to parse the arguments. If it failed, we will check if it was
    // the DisplayVersion error and show our version, otherwise just fallback to
    // clap's handling.
    let args = {
        match Arguments::try_parse() {
            Ok(arguments) => arguments,
            Err(err) => match err.kind() {
                clap::ErrorKind::DisplayVersion => {
                    version::output_version().await;
                    process::exit(0);
                }
                _ => {
                    err.exit();
                }
            },
        }
    };

    if let Err(err) = initialize_logger(&args) {
        eprintln!("unable to initialize logging: {:?}", err);
        process::exit(1);
    };

    // Start the background version check, but skip it when running the `Update`
    // or `Version` command, or if the disable_version_check is set to true.
    let disable_version_check = args.disable_version_check
        || matches!(
            args.sub_command,
            Update(_) | Version(_) | Completions { .. } | Shell { .. }
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
        Experiments(args) => experiments::handle_command(args).await,
        Login => auth::handle_login_command(args).await,
        Logout => auth::handle_logout_command(args).await,
        Labels(args) => labels::handle_command(args).await,
        Notebooks(args) => notebooks::handle_command(args).await,
        Providers(args) => providers::handle_command(args).await,
        Proxies(args) => proxies::handle_command(args).await,
        Run(args) => run::handle_command(args).await,
        Templates(args) => templates::handle_command(args).await,
        Triggers(args) => triggers::handle_command(args).await,
        Events(args) => events::handle_command(args).await,
        Update(args) => update::handle_command(args).await,
        Users(args) => users::handle_command(args).await,
        Version(args) => version::handle_command(args).await,
        Shell(args) => shell::handle_command(args).await,
        Completions { shell } => {
            let output = generate_completions(shell);
            stdout().lock().write_all(output.as_bytes()).unwrap();
            Ok(())
        }
    };

    if let Err(ref err) = result {
        error!("Command did not finish successfully: {:?}", err);
    }

    // Wait for an extra second for the background check to finish
    if let Ok(version_check_result) = timeout(Duration::from_secs(1), version_check_result).await {
        match version_check_result {
            Ok(Some(new_version)) => {
                info!("A new version of fp is available (version: {}). Use `fp update` to update your current fp binary", new_version);
            }
            Ok(None) => trace!("background version check skipped or no new version available"),
            Err(err) => warn!(%err, "background version check failed"),
        }
    }

    if result.is_err() {
        process::exit(1);
    }
}

/// If verbose is set, then we show debug log message from the `fp` target,
/// using a more verbose format.
fn initialize_logger(args: &Arguments) -> Result<()> {
    if args.verbose {
        // If RUST_LOG is set, then use the directives from there, otherwise
        // info as the default level for everything, except for fp, which will
        // use debug.
        let filter = match env::var(EnvFilter::DEFAULT_ENV) {
            Ok(env_var) => EnvFilter::try_new(env_var),
            _ => EnvFilter::try_new("info,fp=debug"),
        }?;

        // Create a more verbose logger that show timestamp, level, and all the
        // fields.
        let tracing = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_env_filter(filter);

        if let Some(path) = &args.log_file {
            tracing
                .with_ansi(false)
                .with_writer(std::fs::File::create(path)?)
                .try_init()
                .expect("unable to initialize logging");
        } else {
            tracing
                .with_writer(io::stderr)
                .try_init()
                .expect("unable to initialize logging");
        }
    } else {
        let filter = match env::var(EnvFilter::DEFAULT_ENV) {
            Ok(env_var) => EnvFilter::try_new(env_var),
            _ => EnvFilter::try_new("fp=info"),
        }?;

        // Create a custom field formatter, which only outputs the `message`
        // field, all other fields are ignored.
        let field_formatter = format::debug_fn(|writer, field, value| {
            if field.name() == "message" {
                write!(writer, "{:?}", value)
            } else {
                Ok(())
            }
        });
        tracing_subscriber::fmt()
            .fmt_fields(field_formatter)
            .without_time()
            .with_level(false)
            .with_max_level(tracing::Level::INFO)
            .with_span_events(format::FmtSpan::NONE)
            .with_target(false)
            .with_writer(io::stderr)
            .with_env_filter(filter)
            .try_init()
            .expect("unable to initialize logging");
    }

    Ok(())
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

pub struct KeyValueArgument {
    pub key: String,
    pub value: String,
}

impl FromStr for KeyValueArgument {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        if s.is_empty() {
            return Err(anyhow!("empty input"));
        }

        let (key, value) = match s.split_once('=') {
            Some((key, value)) => (key, value),
            None => (s, ""),
        };

        Ok(KeyValueArgument {
            key: key.to_owned(),
            value: value.to_owned(),
        })
    }
}

fn generate_completions(shell: Shell) -> String {
    let mut app = Arguments::into_app();
    let app_name = app.get_name().to_string();
    let mut output = Vec::new();
    generate(shell, &mut app, app_name, &mut output);
    let output = String::from_utf8(output).unwrap();
    // There is some bug in the output generated by clap_complete that causes
    // the error "_arguments:comparguments:325: can only be called from completion function"
    // This solution fixes the problem: https://github.com/clap-rs/clap/issues/2488#issuecomment-999227749
    if shell == Shell::Zsh {
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
    }
}

#[test]
fn generating_completions() {
    // Check that this works
    generate_completions(Shell::Bash);

    let zsh_completions = generate_completions(Shell::Zsh);
    assert_eq!(zsh_completions.lines().next().unwrap(), "#compdef _fp fp");
}
