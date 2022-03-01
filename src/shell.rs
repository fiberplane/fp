use std::{
    io::{Read, Write},
    process::Stdio,
    sync::Arc,
};

use crate::config::api_client_configuration;
use crate::utils::notebook_worker::Worker;
use anyhow::{anyhow, Result};
use bytes::BytesMut;
use clap::Parser;
use fiberplane::protocols::core::{Cell, CodeCell};
use pty_process::{Command, Size};
//use portable_pty::{native_pty_system, CommandBuilder, PtySize, PtySystem};
use tracing::{error, info, instrument};

#[derive(Parser)]
pub struct Arguments {
    // ID of the notebook
    #[clap(name = "id")]
    id: String,

    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,
}

#[cfg(target_os = "linux")]
const DEFAULT_SHELL: &str = "/bin/bash";
#[cfg(target_os = "windows")]
const DEFAULT_SHELL: &str = "powershell.exe";

use std::os::unix::io::AsRawFd;

pub struct RawGuard {
    termios: nix::sys::termios::Termios,
}

impl RawGuard {
    #[allow(dead_code)]
    pub fn new() -> Self {
        let stdin = std::io::stdin().as_raw_fd();
        let termios = nix::sys::termios::tcgetattr(stdin).unwrap();
        let mut termios_raw = termios.clone();
        nix::sys::termios::cfmakeraw(&mut termios_raw);
        nix::sys::termios::tcsetattr(stdin, nix::sys::termios::SetArg::TCSANOW, &termios_raw)
            .unwrap();
        Self { termios }
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let stdin = std::io::stdin().as_raw_fd();
        let _ =
            nix::sys::termios::tcsetattr(stdin, nix::sys::termios::SetArg::TCSANOW, &self.termios);
    }
}

use tokio::{
    fs::File,
    io::{AsyncReadExt as _, AsyncWriteExt as _},
};

use futures_retry::{FutureRetry, RetryPolicy};
use std::time::Duration;

#[instrument(err, skip_all)]
pub(crate) async fn handle_command(args: Arguments) -> Result<()> {
    let shell_exe = std::env::var("SHELL").unwrap_or(DEFAULT_SHELL.to_string());

    let w = Arc::new(Worker::new(args.base_url, args.id, args.config).await?);

    let id = w
        .insert_cell(Cell::Code(CodeCell {
            ..Default::default()
        }))
        .await?;

    let mut child = tokio::process::Command::new(&shell_exe).spawn_pty(None)?;

    let _raw = RawGuard::new();

    let mut in_buf = [0_u8; 4096];
    let mut out_buf = [0_u8; 4096];

    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    let mut f = File::create("./log.txt").await?;

    loop {
        tokio::select! {
            bytes = stdin.read(&mut in_buf) => match bytes {
                Ok(bytes) => {
                    let data = &in_buf[..bytes];
                    child.pty_mut().write_all(data).await.unwrap();
                }
                Err(e) => {
                    eprintln!("stdin read failed: {:?}", e);
                    break;
                }
            },
            bytes = {child.pty_mut().read(&mut out_buf)} => match bytes {
                Ok(bytes) => {
                    let data = &out_buf[..bytes];
                    stdout.write_all(data).await.unwrap();
                    stdout.flush().await.unwrap();

                    let data = strip_ansi_escapes::strip(data).unwrap();
                    let utf8 = String::from_utf8_lossy(&data).to_string();
                    f.write_all(utf8.as_str().as_bytes()).await;

                    while let Err(_) = w.append_cell_content(&id, &utf8).await {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
                Err(e) => {
                    eprintln!("pty read failed: {:?}", e);
                    break;
                }
            },
            //_ = child.wait() => break,
        }
    }

    /* let foo = tokio::task::spawn_blocking(move || -> Result<()> {
        info!("Spawned blocking");

        let mut child = std::process::Command::new(&shell_exe)
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .spawn()?;

        info!("Spawned child");
        let reader = child.stdout.as_mut().unwrap();

        let mut buf = BytesMut::with_capacity(1024);
        while let Some(Ok(s)) = child
            .stdout
            .as_mut()
            .map(|reader| reader.read(&mut buf[..]))
        {
            if s == 0 {
                if let Ok(res) = child.try_wait() {
                    if let Some(status) = res {
                        info!(?status, "child exit");
                        break;
                    }
                    continue;
                } else {
                    error!("try_wait fail");
                    break;
                }
            }
            info!("read {} bytes", s);

            let data = buf.split_off(s).freeze();
            //std::io::stdout().write_all(&data);
            tx.try_send(data)?
        }

        child.wait();

        Ok(())
    });

    let reader = tokio::spawn(async move {
        let mut data = vec![];
        while let Ok(bytes) = rx.recv().await {
            data.copy_from_slice(&bytes[..]);
        }

        data
    });

    foo.await?;
    let data = reader.await?;

    info!("Read {} bytes from ptty", data.len()); */

    Ok(())
}
