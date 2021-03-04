use clap::{crate_authors, crate_version, Clap};

mod webhook;
mod ws;

#[derive(Clap)]
#[clap(
    version = crate_version!(),
    author = crate_authors!(),
    about = "Interacts with the Fiberplane API"
)]
struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Clap)]
enum SubCommand {
    #[clap(name = "webhook", about = "Interact with Fiberplane Webhooks")]
    Webhook(webhook::Arguments),

    #[clap(
        name = "web-sockets",
        about = "Interact with the Fiberplane realtime API"
    )]
    WebSockets(ws::Arguments),
}

#[tokio::main]
async fn main() {
    let args = Arguments::parse();

    use SubCommand::*;
    match args.subcmd {
        Webhook(args) => webhook::handle_command(args).await,
        WebSockets(args) => ws::handle_command(args),
    }
}
