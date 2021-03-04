use clap::Clap;

#[derive(Clap)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

pub fn handle_command(args: Arguments) {
    match args.subcmd {
        SubCommand::Monitor(args) => handle_monitor_command(args),
    }
}

#[derive(Clap)]
pub enum SubCommand {
    #[clap(name = "monitor", about = "Monitor a fiberplane realtime connection")]
    Monitor(MonitorArguments),
}

#[derive(Clap)]
pub struct MonitorArguments {
    #[clap(
        name = "notebook",
        long,
        short,
        number_of_values = 1,
        about = "subscribe to these notebooks"
    )]
    notebooks: Vec<String>,
}

pub fn handle_monitor_command(args: MonitorArguments) {
    println!("web-sockets monitor command!");
    for notebook in args.notebooks.iter() {
        println!("Subscribing to notebooks {:?}", notebook);
    }

    todo!()
}
