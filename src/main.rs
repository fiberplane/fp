use clap::{AppSettings, Parser};

mod auth;
mod config;
mod plugins;
mod proxies;
mod webhook;
mod ws;

#[derive(Parser)]
#[clap(author, about, version, setting = AppSettings::PropagateVersion)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,

    #[clap(
        long,
        about = "Base URL for requests to Fiberplane",
        default_value = "https://fiberplane.com",
        env = "API_BASE",
        global = true
    )]
    // TODO parse as a URL
    base_url: String,

    #[clap(long, about = "Path to Fiberplane config.toml file", global = true)]
    // TODO parse this as a PathBuf
    config: Option<String>,
}

#[derive(Parser)]
enum SubCommand {
    #[clap(
        name = "login",
        about = "Login to Fiberplane and authorize the CLI to access your account"
    )]
    Login,

    #[clap(name = "logout", about = "Logout from Fiberplane")]
    Logout,

    #[clap(name = "plugins", about = "Interact with Fiberplane Plugins")]
    Plugins(plugins::Arguments),

    #[clap(name = "webhook", about = "Interact with Fiberplane Webhooks")]
    Webhook(webhook::Arguments),

    #[clap(
        name = "web-sockets",
        aliases = &["web-sockets", "ws"],
        about = "Interact with the Fiberplane realtime API"
    )]
    WebSockets(ws::Arguments),

    #[clap(
        name = "proxies",
        alias = "proxy",
        about = "Commands related to Fiberplane Proxies"
    )]
    Proxies(proxies::Arguments),
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = Arguments::parse();

    use SubCommand::*;
    match args.subcmd {
        // TODO we should make all of the subcommands return anyhow::Error
        Plugins(args) => plugins::handle_command(args).await,
        Webhook(args) => webhook::handle_command(args).await,
        WebSockets(args) => ws::handle_command(args).await,
        Login => auth::handle_login_command(args).await.unwrap(),
        Logout => auth::handle_logout_command(args).await.unwrap(),
        Proxies(args) => proxies::handle_command(args).await.unwrap(),
    }
}
