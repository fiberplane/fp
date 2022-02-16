use clap::{AppSettings, IntoApp, Parser};
use clap_complete::{generate, Shell};
use std::{io, process};

mod auth;
mod config;
mod notebooks;
mod providers;
mod proxies;
mod templates;
mod triggers;

#[derive(Parser)]
#[clap(author, about, version, setting = AppSettings::PropagateVersion)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,

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
}

#[derive(Parser)]
enum SubCommand {
    /// Login to Fiberplane and authorize the CLI to access your account
    #[clap()]
    Login,

    /// Logout from Fiberplane
    #[clap()]
    Logout,

    /// Interact with Fiberplane Providers
    #[clap()]
    Providers(providers::Arguments),

    /// Interact with Fiberplane Triggers
    #[clap(alias = "trigger")]
    Triggers(triggers::Arguments),

    /// Commands related to Fiberplane Proxies
    #[clap(alias = "proxy")]
    Proxies(proxies::Arguments),

    /// Commands related to Fiberplane Templates
    #[clap(alias = "template")]
    Templates(templates::Arguments),

    #[clap(
        name = "notebooks",
        aliases = &["notebook", "n"],
        about = "Commands related to Fiberplane Notebooks"
    )]
    Notebooks(notebooks::Arguments),

    /// Generate fp shell completions for your shell and print to stdout
    Completions {
        #[clap(arg_enum)]
        shell: Shell,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = Arguments::parse();

    use SubCommand::*;
    let result = match args.subcmd {
        Login => auth::handle_login_command(args).await,
        Logout => auth::handle_logout_command(args).await,
        Notebooks(args) => notebooks::handle_command(args).await,
        Providers(args) => providers::handle_command(args).await,
        Proxies(args) => proxies::handle_command(args).await,
        Templates(args) => templates::handle_command(args).await,
        Triggers(args) => triggers::handle_command(args).await,
        Completions { shell } => {
            let mut app = Arguments::into_app();
            let app_name = app.get_name().to_string();
            generate(shell, &mut app, app_name, &mut io::stdout().lock());
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {:?}", e);
        process::exit(1);
    }
}
