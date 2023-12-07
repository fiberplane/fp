use crate::config::{api_client_configuration_from_token, Config};
use crate::Arguments;
use anyhow::Error;
use fiberplane::api_client::logout;
use futures_util::Future;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use qstring::QString;
use std::net::SocketAddr;
use std::pin::Pin;
use tokio::net::TcpListener;
use tokio::select;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Run the OAuth flow and save the API token to the config
///
/// This will run an HTTP server on a random local port and
/// open the login API endpoint in the user's browser. Once
/// the login flow is complete, the browser will redirect back
/// to the local HTTP server with the API token in the query string.
pub async fn handle_login_command(args: Arguments) -> Result<(), Error> {
    let mut config = Config::load(args.config).await?;

    // Note this needs to be a broadcast channel, even though we are only using it once,
    // so that we can move the tx into the service handler closures
    let (tx, mut rx) = broadcast::channel(1);

    // Bind to a random local port
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    // info!("Local redirect server listening on {}", addr.local) TODO!
    let listener = TcpListener::bind(addr).await?;

    let port: u16 = listener.local_addr()?.port();
    // Include the port of the local HTTP server so the
    // API can redirect the browser back to us after the login
    // flow is completed
    let login_url = format!("{}signin?cli_redirect_port={}", args.base_url, port);
    debug!("listening for the login redirect on port {port} (redirect url: {login_url})");

    // Start the local HTTP server
    tokio::task::spawn(async move {
        loop {
            let local_tx = tx.clone();

            let (stream, _) = listener
                .accept()
                .await
                .expect("unable to accept connection");
            let io = TokioIo::new(stream);

            tokio::task::spawn(async move {
                if let Err(err) = http1::Builder::new()
                    .serve_connection(io, LoginRedirectServer { tx: local_tx })
                    .await
                {
                    println!("Error serving connection: {:?}", err);
                }
            });
        }
    });

    // Open the user's web browser to start the login flow
    if webbrowser::open(&login_url).is_err() {
        info!("Please go to this URL to login: {}", login_url);
    }

    // wait for ctrl+c or a token to be received on the token channel
    select! {
        token = rx.recv() => {
            match token {
                Ok(token) => {
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
       }
    };

    Ok(())
}

/// Logout from Fiberplane and delete the API Token from the config file
pub async fn handle_logout_command(args: Arguments) -> Result<(), Error> {
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

    Ok(())
}

struct LoginRedirectServer {
    tx: broadcast::Sender<String>,
}

impl Service<Request<hyper::body::Incoming>> for LoginRedirectServer {
    type Response = Response<Full<Bytes>>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<hyper::body::Incoming>) -> Self::Future {
        let token = req
            .uri()
            .query()
            .and_then(|query| QString::from(query).get("token").map(String::from));

        let res = match token {
            Some(token) => {
                self.tx
                    .send(token)
                    .expect("error sending token via channel");
                Response::builder()
                    .status(StatusCode::OK)
                    .body(Full::new(Bytes::from(
                        "You have been logged in to the CLI. You can now close this tab.",
                    )))
                    .unwrap()
            }
            None => Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::from(
                    "Expected token query string parameter",
                )))
                .unwrap(),
        };

        Box::pin(async { Ok(res) })
    }
}
