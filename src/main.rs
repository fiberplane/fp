use clap::{AppSettings, Clap};

mod auth;
mod config;
mod plugins;
mod webhook;
mod ws;

#[derive(Clap)]
#[clap(author, about, version, setting = AppSettings::GlobalVersion)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,

    #[clap(
        long,
        about = "Base URL for requests to Fiberplane",
        default_value = "https://fiberplane.com",
        env = "API_BASE"
    )]
    base_url: String,

    #[clap(long, about = "Path to Fiberplane config.toml file")]
    config: Option<String>,
}

#[derive(Clap)]
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
    }
}
