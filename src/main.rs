use crate::config::Config;
use crate::fp_urls::NotebookUrlBuilder;
use anyhow::{anyhow, Context, Error, Result};
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser, ValueHint};
use clap_complete::{generate, Shell};
use config::api_client_configuration;
use directories::ProjectDirs;
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::labels::Label;
use fiberplane::models::notebooks::NewNotebook;
use fiberplane::models::timestamps::{NewTimeRange, RelativeTimeRange};
use human_panic::setup_panic;
use interactive::workspace_picker;
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
mod daemons;
mod data_sources;
mod events;
mod experiments;
mod fp_urls;
mod front_matter;
pub(crate) mod integrations;
mod interactive;
mod labels;
mod manifest;
mod notebooks;
mod output;
mod profiles;
mod providers;
mod run;
mod shell;
mod snippets;
mod templates;
mod tokens;
mod triggers;
mod update;
mod users;
mod utils;
mod version;
mod views;
mod webhooks;
mod workspaces;

/// The current build manifest associated with this binary
pub static MANIFEST: Lazy<Manifest> = Lazy::new(Manifest::from_env);

/// The time before the fp command will try to do a version check again, in
/// seconds.
const VERSION_CHECK_DURATION: u64 = 60 * 60 * 24; // 24 hours

#[derive(Parser)]
#[clap(author, about, version, propagate_version = true)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,

    /// Base URL to the Fiberplane API
    #[clap(long, env = "API_BASE", global = true, help_heading = "Global options")]
    base_url: Option<Url>,

    /// Name of the profile to use
    #[clap(long, global = true, env, help_heading = "Global options")]
    profile: Option<String>,

    /// Override the API token used
    ///
    /// If nothing is specified then it will use the token from the profile file.
    #[clap(long, global = true, env = "FP_TOKEN", help_heading = "Global options")]
    token: Option<String>,

    /// Disables the version check
    #[clap(long, global = true, env, help_heading = "Global options")]
    disable_version_check: bool,

    /// Display verbose logs
    #[clap(short, long, global = true, env, help_heading = "Global options")]
    verbose: bool,

    /// Path to log file
    #[clap(long, global = true, env, help_heading = "Global options")]
    log_file: Option<PathBuf>,

    /// Workspace to use
    #[clap(long, short, env, global = true, help_heading = "Global options")]
    workspace_id: Option<Base64Uuid>,
}

#[derive(Parser)]
enum SubCommand {
    /// Interact with data sources
    ///
    /// Create and manage data sources, and list both direct and FPD data sources.
    #[clap(aliases = &["data-source", "datasources", "datasource"])]
    DataSources(data_sources::Arguments),

    /// Experimental commands
    ///
    /// These commands are not stable and may change at any time.
    #[clap(aliases = &["experiment", "x"])]
    Experiments(experiments::Arguments),

    /// Login to Fiberplane and authorize the CLI to access your account
    Login,

    /// Logout from Fiberplane
    Logout,

    /// Interact with labels
    ///
    /// Labels allow you to organize your notebooks.
    #[clap(alias = "label")]
    Labels(labels::Arguments),

    /// Create a new notebook and open it in the browser.
    ///
    /// If you need access to the json use the `notebook create` command.
    #[clap(alias = "create")]
    New(NewArguments),

    /// Interact with notebooks
    ///
    /// Notebooks are the main resource that Studio exposes.
    #[clap(aliases = &["notebook", "nb"])]
    Notebooks(notebooks::Arguments),

    /// Interact with front matter collections
    ///
    /// Front matter collections are pre-determined sets of front matter metadata
    /// that is used to attach metadata to notebooks.
    #[clap(aliases = &["fmc", "frontmatter"])]
    FrontMatterCollection(front_matter::Arguments),

    /// Interact with providers
    ///
    /// Providers are wasm files that contain the logic to retrieve data based
    /// on a query. This is being used by Studio and FPD.
    #[clap(alias = "provider")]
    Providers(providers::Arguments),

    /// Interact with Fiberplane Daemon instances
    ///
    /// The Fiberplane Daemon allows you to expose services that are hosted
    /// within your network without exposing them or sharing credentials.
    #[clap(aliases = &["daemon", "proxy", "proxies"])]
    Daemons(daemons::Arguments),

    /// Run a command and send the output to a notebook
    ///
    /// Note: to run a command with pipes, you must wrap the command in quotes.
    /// For example, `fp run "echo hello world | grep hello"`
    #[clap(trailing_var_arg = true)]
    Run(run::Arguments),

    /// Interact with templates
    ///
    /// Templates allow you to create notebooks based on jsonnet.
    #[clap(alias = "template")]
    Templates(templates::Arguments),

    /// Launch a recorded shell session that'll show up in the notebook
    Shell(shell::Arguments),

    /// Snippets allow you to save reusable groups of cells and insert them into notebooks.
    #[clap(alias = "snippet")]
    Snippets(snippets::Arguments),

    /// Views allow you to save label searches and display them as a view, allowing you to search for
    /// notebooks easier and more convenient
    #[clap(alias = "view")]
    Views(views::Arguments),

    /// Interact with triggers
    ///
    /// Triggers allow you to expose webhooks that will expand templates.
    /// This could be used for alertmanager, for example.
    #[clap(alias = "trigger")]
    Triggers(triggers::Arguments),

    /// Interact with events
    ///
    /// Events allow you to mark a specific point in time when something occurred, such as a deployment.
    #[clap(alias = "event")]
    Events(events::Arguments),

    /// Interact with API tokens
    #[clap(alias = "token")]
    Tokens(tokens::Arguments),

    /// Update the current FP binary
    Update(update::Arguments),

    /// Interact with user details
    #[clap(alias = "user")]
    Users(users::Arguments),

    /// Interact with workspaces
    ///
    /// A workspace holds all notebooks, events and relays for a specific user or organization.
    #[clap(alias = "workspace")]
    Workspaces(workspaces::Arguments),

    /// Interact with webhooks
    ///
    /// Webhooks allow you to receive http requests from Fiberplane when certain events occur
    #[clap(aliases = &["webhook", "wh"])]
    Webhooks(webhooks::Arguments),

    /// Interact with personal integrations.
    ///
    /// Integrations allow you to integrate various third-party tools into Fiberplane.
    ///
    /// If you wish to configure workspace level integrations, please use `fp workspaces integrations`
    #[clap(alias = "integration")]
    Integrations(integrations::Arguments),

    /// Profiles allow you to manage different `fp` values such as `base_url` and `token`
    /// and switch between them on demand
    #[clap(alias = "profile")]
    Profiles(profiles::Arguments),

    /// Display extra version information
    Version(version::Arguments),

    /// Generate fp shell completions for your shell and print to stdout
    #[clap(hide = true)]
    Completions {
        #[clap(value_enum)]
        shell: Shell,
    },

    /// Generate markdown reference for fp.
    #[clap(hide = true)]
    Markdown,
}

#[tokio::main]
async fn main() {
    // Set the human panic handler first thing first so in case we have a panic when setting
    // up the CLI, it also gets caught
    setup_panic!(Metadata {
        name: "fp".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        authors: "issues@fiberplane.com".into(),
        homepage: "https://fiberplane.com".into(),
    });

    let mut cli_args = env::args();

    // skip program name
    let _ = cli_args.next();

    // check if the second argument specifies a profile
    let maybe_profile = cli_args.next();
    let profile = maybe_profile
        .as_ref()
        .and_then(|input| input.strip_prefix('+'));

    let clap_args = if let Some(profile) = profile {
        let mut vec: Vec<_> = env::args_os()
            .take(1)
            .chain(env::args_os().skip(2))
            .collect();

        vec.push(format!("--profile={profile}").into());
        vec
    } else {
        env::args_os().collect()
    };

    // We would like to override the builtin version display behavior, so we
    // will try to parse the arguments. If it failed, we will check if it was
    // the DisplayVersion error and show our version, otherwise just fallback to
    // clap's handling.
    let args = match Parser::try_parse_from(clap_args) {
        Ok(args) => args,
        Err(err) => match err.kind() {
            ErrorKind::DisplayVersion => {
                version::output_version().await;
                process::exit(0);
            }
            _ => {
                err.exit();
            }
        },
    };

    // load the config and or migrate from previous version
    if let Err(err) = config::migrate().await {
        eprintln!("failed to load migrate config into profile: {err:?}");
        process::exit(1);
    }

    if let Err(err) = initialize_logger(&args) {
        eprintln!("unable to initialize logging: {err:?}");
        process::exit(1);
    }

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
        DataSources(args) => data_sources::handle_command(args).await,
        Experiments(args) => experiments::handle_command(args).await,
        Login => auth::handle_login_command(args).await,
        Logout => auth::handle_logout_command(args).await,
        Labels(args) => labels::handle_command(args).await,
        New(args) => handle_new_command(args).await,
        Notebooks(args) => notebooks::handle_command(args).await,
        FrontMatterCollection(args) => front_matter::handle_command(args).await,
        Providers(args) => providers::handle_command(args).await,
        Daemons(args) => daemons::handle_command(args).await,
        Run(args) => run::handle_command(args).await,
        Shell(args) => shell::handle_command(args).await,
        Snippets(args) => snippets::handle_command(args).await,
        Views(args) => views::handle_command(args).await,
        Templates(args) => templates::handle_command(args).await,
        Triggers(args) => triggers::handle_command(args).await,
        Events(args) => events::handle_command(args).await,
        Tokens(args) => tokens::handle_command(args).await,
        Update(args) => update::handle_command(args).await,
        Users(args) => users::handle_command(args).await,
        Workspaces(args) => workspaces::handle_command(args).await,
        Webhooks(args) => webhooks::handle_command(args).await,
        Integrations(args) => integrations::handle_command(args).await,
        Profiles(args) => profiles::handle_command(args).await,
        Version(args) => version::handle_command(args).await,
        Completions { shell } => {
            let output = generate_completions(shell);
            stdout().lock().write_all(output.as_bytes()).unwrap();
            Ok(())
        }
        Markdown => {
            clap_markdown::print_help_markdown::<Arguments>();
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
                write!(writer, "{value:?}")
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

#[derive(Clone)]
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

impl From<KeyValueArgument> for Label {
    fn from(kv: KeyValueArgument) -> Self {
        Self::new(kv.key, kv.value)
    }
}

fn generate_completions(shell: Shell) -> String {
    let mut app = Arguments::command();
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

#[derive(Parser)]
struct NewArguments {
    /// Workspace to use
    #[clap(long, short, env)]
    workspace_id: Option<Base64Uuid>,

    /// Title for the new notebook
    #[clap(trailing_var_arg(true), value_hint = ValueHint::CommandWithArguments, num_args = 0..)]
    title: Vec<String>,

    #[clap(from_global)]
    base_url: Option<Url>,

    #[clap(from_global)]
    profile: Option<String>,

    #[clap(from_global)]
    token: Option<String>,
}

async fn handle_new_command(args: NewArguments) -> Result<()> {
    let config = Config::load(args.profile.clone()).await?;
    let client = api_client_configuration(args.token, args.profile, args.base_url.clone()).await?;

    let workspace_id = workspace_picker(&client, args.workspace_id).await?;
    let title = if args.title.is_empty() {
        "Untitled".to_string()
    } else {
        args.title.join(" ")
    };

    let new_notebook = NewNotebook::builder()
        .title(title)
        .time_range(NewTimeRange::Relative(RelativeTimeRange::from_minutes(60)))
        .build();
    let notebook = client.notebook_create(workspace_id, new_notebook).await?;

    let notebook_id = Base64Uuid::parse_str(&notebook.id)?;

    let notebook_url = NotebookUrlBuilder::new(workspace_id, notebook_id)
        .base_url(config.base_url(args.base_url)?)
        .url()?;

    // Open the user's web browser
    if webbrowser::open(notebook_url.as_str()).is_err() {
        eprintln!("Unable to open the web browser");
    }

    println!("{notebook_url}");

    Ok(())
}
