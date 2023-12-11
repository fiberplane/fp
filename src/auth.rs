use crate::config::{api_client_configuration_from_token, Config};
use crate::Arguments;
use anyhow::Error;
use fiberplane::api_client::logout;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Response, Server, StatusCode};
use qstring::QString;
use std::convert::Infallible;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

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
    let login_url = format!("{}signin?cli_redirect_port={}", args.base_url, port);

    debug!("listening for the login redirect on port {port} (redirect url: {login_url})");

    // Open the user's web browser to start the login flow
    if webbrowser::open(&login_url).is_err() {
        info!("Please go to this URL to login: {}", login_url);
    }

    let mut config = Config::load(args.config).await?;

    // Shut down the web server once the token is received
    server
        .with_graceful_shutdown(async move {
            // Wait for the token to be received
            match rx.recv().await {
                Ok(token) => {
                    debug!("api token: {}", token);

                    // Save the token to the config file
                    config.api_token = Some(token);
                    match config.save().await {
                        Ok(_) => {
                            info!("You are logged in to Fiberplane");
                        }
                        Err(e) => error!(
                            "Error saving API token to config file {}: {:?}",
                            config.path.display(),
                            e
                        ),
                    };
                }
                Err(_) => error!("login error"),
            }
        })
        .await?;

    Ok(())
}

/// Invalidate the API token.
///
/// If a token is set using the `--token` flag, then that will be used,
/// otherwise it will use the token from the config file. The token will also be
/// removed from the config file if that was used.
pub async fn handle_logout_command(args: Arguments) -> Result<(), Error> {
    if let Some(token) = args.token {
        let api_config = api_client_configuration_from_token(&token, args.base_url)?;

        logout(&api_config).await?;

        info!("You are logged out");
    } else {
        let mut config = Config::load(args.config).await?;

        match config.api_token {
            Some(token) => {
                let api_config = api_client_configuration_from_token(&token, args.base_url)?;

                logout(&api_config).await?;

                config.api_token = None;
                config.save().await?;

                info!("You are logged out");
            }
            None => {
                warn!("You are already logged out");
            }
        }
    }

    Ok(())
}
