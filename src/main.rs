use clap::{AppSettings, Clap};
use simple_logger::SimpleLogger;

mod auth;
mod plugins;
mod webhook;
mod ws;

#[derive(Clap)]
#[clap(author, about, version, setting = AppSettings::GlobalVersion)]
struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Clap)]
enum SubCommand {
    // TODO auth probably shouldn't be a subcommand (or at least we should support `fp login`)
    #[clap(name = "auth", about = "Login to Fiberplane")]
    Auth(auth::Arguments),

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
    // TODO decide which log library to use
    SimpleLogger::new().init().unwrap();

    let args = Arguments::parse();

    use SubCommand::*;
    match args.subcmd {
        Plugins(args) => plugins::handle_command(args).await,
        Webhook(args) => webhook::handle_command(args).await,
        WebSockets(args) => ws::handle_command(args).await,
        // TODO we should make all of the subcommands return anyhow::Error
        Auth(args) => auth::handle_command(args).await.unwrap(),
    }
}
