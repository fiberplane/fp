use crate::config::{api_client_configuration_from_token, Config};
use crate::Arguments;
use anyhow::Error;
use bytes::Bytes;
use http_body_util::Full;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Response, StatusCode};
use hyper_util::rt::TokioIo;
use qstring::QString;
use std::convert::Infallible;
use tokio::net::TcpListener;
use tokio::spawn;
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
    let tcp_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();

    // Include the port of the local HTTP server so the
    // API can redirect the browser back to us after the login
    // flow is completed
    let port: u16 = tcp_listener.local_addr().unwrap().port();
    let login_url = format!("{}signin?cli_redirect_port={}", args.base_url, port);

    // Spawn web server which will handle a redirect from the login page.
    spawn(async move {
        loop {
            let (stream, _) = tcp_listener
                .accept()
                .await
                .expect("unable to accept connection");

            let tx = tx.clone();

            // Use an adapter to access something implementing `tokio::io` traits as if they implement
            // `hyper::rt` IO traits.
            let io = TokioIo::new(stream);

            // Spawn a tokio task to serve multiple connections concurrently
            tokio::task::spawn(async move {
                // Finally, we bind the incoming connection to our `hello` service
                if let Err(err) = http1::Builder::new()
                    // `service_fn` converts our function in a `Service`
                    .serve_connection(
                        io,
                        service_fn(move |req| {
                            let token = req
                                .uri()
                                .query()
                                .and_then(|query| QString::from(query).get("token").map(String::from));
                            let tx = tx.clone();

                            async move {
                                match token {
                                    Some(token) => {
                                        tx.send(token).expect("error sending token via channel");
                                        Ok::<Response<Full<Bytes>>, Infallible>(
                                            Response::builder()
                                                .status(StatusCode::OK)
                                                .body(Full::new(Bytes::from("You have been logged in to the CLI. You can now close this tab.")))
                                                .unwrap(),
                                        )
                                    }
                                    None => Ok::<Response<Full<Bytes>>, Infallible>(
                                        Response::builder()
                                            .status(StatusCode::BAD_REQUEST)
                                            .body(Full::new(Bytes::from("Expected token query string parameter.")))
                                            .unwrap(),
                                    ),
                                }
                            }
                        }),
                    )
                    .await
                {
                    println!("Error serving connection: {:?}", err);
                }
            });
        }
    });

    debug!("listening for the login redirect on port {port} (redirect url: {login_url})");

    // Open the user's web browser to start the login flow
    if webbrowser::open(&login_url).is_err() {
        info!("Please go to this URL to login: {}", login_url);
    }

    // Wait on the channel. Once we receive something it means that the user has
    // logged in.
    match rx.recv().await {
        Ok(token) => {
            let mut config = Config::load(args.config).await?;

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
    };

    Ok(())
}

/// Invalidate the API token.
///
/// If a token is set using the `--token` flag, then that will be used,
/// otherwise it will use the token from the config file. The token will also be
/// removed from the config file if that was used.
pub async fn handle_logout_command(args: Arguments) -> Result<(), Error> {
    if let Some(token) = args.token {
        let client = api_client_configuration_from_token(&token, args.base_url)?;

        client.logout().await?;

        info!("You are logged out");
    } else {
        let mut config = Config::load(args.config).await?;

        match config.api_token {
            Some(token) => {
                let client = api_client_configuration_from_token(&token, args.base_url)?;

                client.logout().await?;

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
