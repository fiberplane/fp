use crate::utils::ansi::Action;
use crate::utils::notebook_worker::Worker;
use crate::{config::api_client_configuration, utils::ansi::Collector};
use anes::parser::{KeyCode, KeyModifiers};
use anyhow::{anyhow, Result};
use clap::Parser;
use fiberplane::protocols::core::{Cell, CodeCell};
use portable_pty::{native_pty_system, CommandBuilder, PtySize, PtySystem};
//use pty_process::Command;
use async_compat::CompatExt;
use blocking::{unblock, Unblock};
use std::cmp;
use std::collections::VecDeque;
use std::os::unix::io::AsRawFd;
use std::time::Duration;
use std::{
    io::{Read, Write},
    process::Stdio,
    sync::Arc,
};
use tokio::io::AsyncReadExt;
use tokio::{
    fs::File,
    io::{AsyncReadExt as _, AsyncWriteExt as _},
};
use tracing::{error, info, instrument};

#[derive(Parser)]
pub struct Arguments {
    // ID of the notebook
    #[clap(name = "id", env = "__FP_NOTEBOOK_ID")]
    id: String,

    #[clap(default_value_t = false, parse(from_flag), env = "__FP_SHELL_SESSION")]
    nested: bool,

    #[clap(from_global)]
    base_url: String,

    #[clap(from_global)]
    config: Option<String>,
}

#[cfg(target_os = "linux")]
const DEFAULT_SHELL: &str = "/bin/bash";
#[cfg(target_os = "macos")]
const DEFAULT_SHELL: &str = "/bin/zsh";
#[cfg(target_os = "windows")]
const DEFAULT_SHELL: &str = "powershell.exe";

pub struct RawGuard {
    was_enabled: bool,
}

impl RawGuard {
    #[allow(dead_code)]
    pub fn new() -> Self {
        if crossterm::terminal::is_raw_mode_enabled().unwrap() {
            Self { was_enabled: true }
        } else {
            crossterm::terminal::enable_raw_mode().unwrap();
            Self { was_enabled: false }
        }
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        if !self.was_enabled {
            crossterm::terminal::disable_raw_mode().unwrap();
        }
    }
}

#[instrument(err, skip_all)]
pub(crate) async fn handle_command(args: Arguments) -> Result<()> {
    use anes::parser::Sequence::*;
    const NONE: KeyModifiers = KeyModifiers::empty();
    let shell_exe = std::env::var("SHELL").unwrap_or(DEFAULT_SHELL.to_string());

    let w = Arc::new(Worker::new(args.base_url.clone(), args.id.clone(), args.config).await?);

    let _raw = RawGuard::new();

    let mut in_buf = [0_u8; 4096];
    let mut out_buf = [0_u8; 4096];

    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    let mut f = File::create("./log.txt").await?;

    let mut statemachine = vte::Parser::new();

    let mut parser = anes::parser::Parser::default();

    let pty_system = native_pty_system();

    let (cols, rows) = crossterm::terminal::size()?;

    let pair = pty_system.openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = CommandBuilder::new(shell_exe);
    cmd.cwd(std::env::current_dir()?);
    cmd.env("__FP_NOTEBOOK_ID", &args.id);
    cmd.env("__FP_SHELL_SESSION", "1");

    // Move the slave to another thread to block and spawn a
    // command.
    // Note that this implicitly drops slave and closes out
    // file handles which is important to avoid deadlock
    // when waiting for the child process!
    let slave = pair.slave;
    let mut child = tokio::task::spawn_blocking(move || slave.spawn_command(cmd)).await??;

    let mut reader = Unblock::new(pair.master.try_clone_reader()?).compat();
    let mut writer = Unblock::new(pair.master.try_clone_writer()?).compat();

    let mut child_waiter = tokio::task::spawn_blocking(move || child.wait());

    let mut buffer = String::new();
    let mut cursor = buffer.len();

    let id = w
        .insert_cell(Cell::Code(CodeCell {
            read_only: Some(true),
            ..Default::default()
        }))
        .await?;

    loop {
        tokio::select! {
            _ = &mut child_waiter => break,
            bytes = stdin.read(&mut in_buf) => match bytes {
                Ok(bytes) => {
                    let data = &in_buf[..bytes];
                    writer.write_all(data).await.unwrap();
                }
                Err(e) => {
                    eprintln!("stdin read failed: {:?}", e);
                    break;
                }
            },
            bytes = {reader.read(&mut out_buf)} => match bytes {
                Ok(bytes) => {
                    let data = &out_buf[..bytes];
                    stdout.write_all(data).await.unwrap();
                    stdout.flush().await.unwrap();
                    /* parser.advance(data, false);

                    let mut min_cursor = cursor;
                    let mut max_cursor = cursor;

                    for seq in &mut parser {
                        match seq {
                            Key(KeyCode::Char(c), NONE) => {
                                buffer.insert(cursor, c);
                                cursor += 1;
                            }
                            Key(KeyCode::Enter, NONE) => {
                                buffer.insert(cursor, '\n');
                                cursor += 1;
                            }
                            Key(KeyCode::Backspace, NONE) => {
                                buffer.remove(if cursor > buffer.len() {
                                    cursor - 1
                                } else {
                                    cursor
                                });
                                cursor -= 1;
                            }
                            Key(KeyCode::Left, NONE) => {
                                cursor -= 1;
                            }
                            Key(KeyCode::Right, NONE) => {
                                cursor += 1;
                            },
                            CursorPosition(x, y) => {

                            }
                            _ => {},
                        }

                        min_cursor = cmp::min(cursor, min_cursor);
                        max_cursor = cmp::max(cursor, max_cursor);
                    }

                    w.replace_cell_content(&id, &buffer[min_cursor..max_cursor], min_cursor..max_cursor).await?; */
                    let stripped = strip_ansi_escapes::strip(data)?;
                    let utf8 = String::from_utf8_lossy(&stripped);
                    w.append_cell_content(&id, &utf8);
                }
                Err(e) => {
                    eprintln!("pty read failed: {:?}", e);
                    break;
                }
            },
            else => break,
        }
    }

    Ok(())
}
