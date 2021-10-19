use clap::Parser;
use fiberplane::protocols::realtime;
use futures_util::{pin_mut, SinkExt, StreamExt};
use tokio::io::AsyncWriteExt;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

pub async fn handle_command(args: Arguments) {
    match args.subcmd {
        SubCommand::Monitor(args) => handle_monitor_command(args).await,
    }
}

#[derive(Parser)]
pub enum SubCommand {
    #[clap(name = "monitor", about = "Monitor a fiberplane realtime connection")]
    Monitor(MonitorArguments),
}

#[derive(Parser)]
pub struct MonitorArguments {
    #[clap(
        long,
        short,
        default_value = "ws://localhost:3030/api/ws",
        env = "WS_ENDPOINT"
    )]
    endpoint: String,

    #[clap(long, short, number_of_values = 1, about = "bearer token")]
    token: String,

    #[clap(
        name = "notebook",
        long,
        short,
        number_of_values = 1,
        about = "subscribe to these notebooks"
    )]
    notebooks: Vec<String>,
}

pub async fn handle_monitor_command(args: MonitorArguments) {
    eprintln!("Connecting to {:?}", args.endpoint);
    let url = url::Url::parse(&args.endpoint).unwrap();

    let (ws_stream, _) = connect_async(url)
        .await
        .expect("unable to connect to web socket server");

    let (mut write, read) = ws_stream.split();

    // First message must be Authenticate.
    let message = realtime::AuthenticateMessage {
        op_id: Some("auth".into()),
        token: args.token,
    };
    let message = realtime::ClientRealtimeMessage::Authenticate(message);
    let message = serde_json::to_string(&message).unwrap();
    write
        .send(Message::Text(message))
        .await
        .expect("send auth did not succeed");

    if !args.notebooks.is_empty() {
        let notebooks = args.notebooks.join(", ");
        eprintln!("Subscribing to notebooks: {:?}", notebooks);
    }
    for notebook in args.notebooks.into_iter() {
        let message = realtime::SubscribeMessage {
            op_id: Some(format!("sub_{:?}", notebook)),
            notebook_id: notebook,
        };
        let message = realtime::ClientRealtimeMessage::Subscribe(message);
        let message = serde_json::to_string(&message).unwrap();
        write
            .send(Message::Text(message))
            .await
            .expect("send did not succeed");
    }

    eprintln!("Requesting debug information");
    let message = realtime::DebugRequestMessage {
        op_id: Some("debug_request".into()),
    };
    let message = realtime::ClientRealtimeMessage::DebugRequest(message);
    let message = serde_json::to_string(&message).unwrap();
    write
        .send(Message::Text(message))
        .await
        .expect("send did not succeed");

    let ws_to_stdout = {
        read.for_each(|message| async {
            match message.unwrap() {
                Message::Text(message) => {
                    tokio::io::stdout()
                        .write_all(message.as_bytes())
                        .await
                        .unwrap();
                    tokio::io::stdout().write(b"\n").await.unwrap();
                }
                Message::Binary(_) => eprintln!("Received unexpected binary content"),
                Message::Ping(_) => eprintln!("Received ping message"),
                Message::Pong(_) => eprintln!("Received pong message"),
                Message::Close(_) => eprintln!("Received close message"),
            };
        })
    };

    pin_mut!(ws_to_stdout);
    ws_to_stdout.await
}
