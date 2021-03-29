use clap::{AppSettings, Clap};

mod templates;
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
    #[clap(
        name = "templates",
        aliases = &["templates", "t"],
        about = "Interact with Fiberplane templates",
    )]
    Templates(templates::Arguments),

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
    let args = Arguments::parse();

    use SubCommand::*;
    match args.subcmd {
        Templates(args) => templates::handle_command(args).await,
        Webhook(args) => webhook::handle_command(args).await,
        WebSockets(args) => ws::handle_command(args).await,
    }
}
