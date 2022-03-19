use crate::utils::ansi::Action;
use crate::utils::notebook_worker::Worker;
use anyhow::{anyhow, Result};
use clap::Parser;
use fiberplane::protocols::core::{Cell, CodeCell};
use portable_pty::{native_pty_system, CommandBuilder, PtySize, PtySystem};
//use pty_process::Command;
use async_compat::CompatExt;
use blocking::{unblock, Unblock};
use std::ops::Range;
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
use wezterm_term::{Line, Screen, TerminalConfiguration};

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

#[derive(Debug)]
struct TermSettings;

impl TerminalConfiguration for TermSettings {
    fn color_palette(&self) -> wezterm_term::color::ColorPalette {
        wezterm_term::color::ColorPalette::default()
    }
}
fn scroll_back_lines(screen: &Screen) -> impl Iterator<Item = &Line> {
    screen
        .lines
        .iter()
        .take(screen.lines.len() - screen.physical_rows)
}

fn scroll_back_char_len(screen: &Screen) -> u32 {
    scroll_back_lines(screen)
        .map(|l| l.as_str().trim_end().len() as u32)
        .sum()
}

fn visible_lines(screen: &Screen) -> impl Iterator<Item = &Line> {
    let line_idx = screen.lines.len() - screen.physical_rows;
    screen
        .lines
        .iter()
        .skip(line_idx)
        .take(screen.physical_rows)
}

#[instrument(err, skip_all)]
pub(crate) async fn handle_command(args: Arguments) -> Result<()> {
    let shell_exe = std::env::var("SHELL").unwrap_or(DEFAULT_SHELL.to_string());

    let w = Arc::new(Worker::new(args.base_url.clone(), args.id.clone(), args.config).await?);

    let _raw = RawGuard::new();

    let mut in_buf = [0_u8; 4096];
    let mut out_buf = [0_u8; 4096];

    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    let mut f = File::create("./log.txt").await?;

    let pty_system = native_pty_system();

    let (cols, rows) = crossterm::terminal::size()?;
    let cols = cols.min(80);
    let rows = rows.min(120);

    let mut term = wezterm_term::Terminal::new(
        wezterm_term::TerminalSize {
            physical_rows: rows as usize,
            physical_cols: cols as usize,
            pixel_width: cols as usize * 4,
            pixel_height: rows as usize * 8,
        },
        Arc::new(TermSettings {}),
        &"xterm",
        "",
        Box::new(Vec::new()),
    );

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

    let id = w
        .insert_cell(Cell::Code(CodeCell {
            read_only: Some(true),
            ..Default::default()
        }))
        .await?;

    let mut buffer = String::new();
    let mut line2range: Vec<Range<usize>> = vec![];

    let mut interval = tokio::time::interval(Duration::from_millis(250));
    let mut seqno = term.current_seqno();

    let mut parser = termwiz::escape::parser::Parser::new();

    let mut buf = String::with_capacity((rows * (cols + 1)) as usize);

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

                    for a in parser.parse_as_vec(data) {
                        f.write_all(format!("{:?}\n", a).as_bytes()).await?;
                    }

                    term.advance_bytes(data);

                }
                Err(e) => {
                    eprintln!("pty read failed: {:?}", e);
                    break;
                }
            },
            _ = interval.tick() => {
                let screen = term.screen();
                buf.clear();
                for l in visible_lines(screen) {
                    buf.push_str(l.as_str().trim_end());
                    buf.push('\n');
                }

                let offset = scroll_back_char_len(screen);

                w.replace_cell_content(&id, &buf, offset).await;
                seqno = term.current_seqno();
            },
            else => break,
        }
    }

    /* f.write_all(
        term.screen()
            .lines
            .iter()
            .map(|l| l.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .as_bytes(),
    )
    .await?; */

    Ok(())
}
