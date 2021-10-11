use anyhow::Error;
use clap::Clap;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Response, Server, StatusCode};
use log::{error, info};
use std::convert::Infallible;
use tokio::sync::broadcast;
use webbrowser;

#[derive(Clap)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Clap, Debug)]
pub enum SubCommand {
    #[clap(
        name = "login",
        about = "Login to Fiberplane and authorize the CLI to access your account"
    )]
    Login(LoginArguments),
}

#[derive(Clap, Debug)]
pub struct LoginArguments {
    #[clap(long, short, default_value = "http://localhost:3030/api")]
    api_base: String,
}

pub async fn handle_command(args: Arguments) -> Result<(), Error> {
    match args.subcmd {
        SubCommand::Login(args) => handle_login_command(args).await,
    }
}

pub async fn handle_login_command(args: LoginArguments) -> Result<(), Error> {
    let (tx, mut rx) = broadcast::channel::<Result<String, ()>>(1);

    // Bind to a random local port
    let redirect_server_addr = ([127, 0, 0, 1], 0).into();
    let make_service = make_service_fn(move |_| {
        let tx = tx.clone();

        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                // TODO proper request handling
                let token = req
                    .uri()
                    .query()
                    .unwrap()
                    .strip_prefix("token=")
                    .unwrap()
                    .to_string();
                dbg!(&token);
                tx.clone().send(Ok(token)).unwrap();

                async move {
                    Ok::<_, Error>(
                        Response::builder()
                            .status(StatusCode::OK)
                            .body(Body::empty())
                            .unwrap(),
                    )
                }
            }))
        }
    });
    let server = Server::bind(&redirect_server_addr).serve(make_service);

    // Include the port of the local HTTP server so the
    // API can redirect the browser back to us after the login
    // flow is completed
    let port: u16 = server.local_addr().port();
    let login_url = format!(
        "{}/oidc/authorize/google?cli_redirect_port={}",
        args.api_base, port
    );

    // Open the user's web browser to start the login flow
    webbrowser::open(&login_url)?;

    // Shut down the web server once the token is received
    server
        .with_graceful_shutdown(async {
            match rx.recv().await.unwrap() {
                Ok(token) => info!("login token: {}", token),
                Err(_) => error!("login error"),
            }
        })
        .await?;

    // TODO redirect to a page that closes the browser tab

    Ok(())
}
