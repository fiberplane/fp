use crate::{config::Config, Arguments};
use anyhow::Error;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Response, Server, StatusCode};
use qstring::QString;
use reqwest::Client;
use std::convert::Infallible;
use tokio::sync::broadcast;
use tracing::{debug, error, info};

/// Run the OAuth flow and save the API token to the config
///
/// This will run an HTTP server on a random local port and
/// open the login API endpoint in the user's browser. Once
/// the login flow is complete, the browser will redirect back
/// to the local HTTP server with the API token in the query string.
pub async fn handle_login_command(args: Arguments) -> Result<(), Error> {
    // Note this needs to be a broadcast channel, even though we are only using it once,
    // so that we can move the tx into the service handler closures
    let (tx, mut rx) = broadcast::channel(1);

    // Bind to a random local port
    let redirect_server_addr = ([127, 0, 0, 1], 0).into();
    let make_service = make_service_fn(move |_| {
        let tx = tx.clone();

        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let token = req
                    .uri()
                    .query()
                    .and_then(|query| QString::from(query).get("token").map(String::from));
                let tx = tx.clone();

                async move {
                    match token {
                        Some(token) => {
                            tx.send(token).expect("error sending token via channel");
                            Ok::<_, Error>(
                                Response::builder()
                                    .status(StatusCode::OK)
                                    .body(Body::from(
                                        "You have been logged in to the CLI. You can now close this tab.",
                                    ))
                                    .unwrap(),
                            )
                        }
                        None => Ok::<_, Error>(
                            Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(Body::from("Expected token query string parameter"))
                                .unwrap(),
                        ),
                    }
                }
            }))
        }
    });
    let server = Server::bind(&redirect_server_addr).serve(make_service);

    // Include the port of the local HTTP server so the
    // API can redirect the browser back to us after the login
    // flow is completed
    let port: u16 = server.local_addr().port();
    debug!("listening for the login redirect on port {}", port);
    let login_url = format!(
        "{}/oidc/authorize/google?cli_redirect_port={}",
        args.api_base, port
    );

    // Open the user's web browser to start the login flow
    if webbrowser::open(&login_url).is_err() {
        println!("Please go to this URL to login: {}", login_url);
    }

    let mut config = Config::load(args.config.as_deref()).await?;

    // Shut down the web server once the token is received
    server
        .with_graceful_shutdown(async move {
            // Wait for the token to be received
            match rx.recv().await {
                Ok(token) => {
                    debug!("api token: {}", token);

                    // Save the token to the config file
                    config.api_token = Some(token);
                    if let Err(e) = config.save().await {
                        eprintln!("Error saving API token to config file: {:?}", e);
                    };
                    info!("saved config to: {}", config.path.as_path().display());
                }
                Err(_) => error!("login error"),
            }
        })
        .await?;

    println!("You are logged in to Fiberplane");

    Ok(())
}

/// Logout from Fiberplane and delete the API Token from the config file
pub async fn handle_logout_command(args: Arguments) -> Result<(), Error> {
    let mut config = Config::load(args.config.as_deref()).await?;

    match config.api_token {
        Some(api_token) => {
            Client::new()
                .post(format!("{}/logout", &args.api_base))
                .body(Body::empty())
                .bearer_auth(api_token)
                .send()
                .await?
                .error_for_status()?;

            config.api_token = None;
            config.save().await?;
            println!("Logged out");
        }
        None => {
            println!("Already logged out");
        }
    }

    Ok(())
}
