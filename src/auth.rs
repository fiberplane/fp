use crate::Arguments;
use anyhow::Error;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Response, Server, StatusCode};
use std::convert::Infallible;
use tokio::sync::broadcast;
use tracing::{debug, error, info};
use webbrowser;

pub async fn handle_login_command(args: Arguments) -> Result<(), Error> {
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
                tx.send(Ok(token)).unwrap();

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
    info!("listening for the login redirect on port {}", port);
    let login_url = format!(
        "{}/oidc/authorize/google?cli_redirect_port={}",
        args.api_base, port
    );

    // Open the user's web browser to start the login flow
    if let Err(_) = webbrowser::open(&login_url) {
        println!("Please go to this URL to login: {}", login_url);
    }

    // Shut down the web server once the token is received
    server
        .with_graceful_shutdown(async {
            match rx.recv().await.unwrap() {
                Ok(token) => debug!("login token: {}", token),
                Err(_) => error!("login error"),
            }
        })
        .await?;

    // TODO redirect to a page that closes the browser tab

    Ok(())
}
