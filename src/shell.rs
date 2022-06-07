use crate::config::api_client_configuration;
use anyhow::{anyhow, Result};
use async_compat::CompatExt;
use blocking::Unblock;
use clap::Parser;
use fp_api_client::apis::default_api::{
    get_profile, notebook_cell_append_text, notebook_cells_append,
};
use fp_api_client::models::cell::HeadingType;
use fp_api_client::models::{Annotation, Cell, CellAppendText};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::path::PathBuf;
use std::time::Duration;
use termwiz::escape::csi::{DecPrivateMode, DecPrivateModeCode, Mode};
use termwiz::escape::{Action, ControlCode, CSI};
use time::OffsetDateTime;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::instrument;

#[derive(Parser)]
pub struct Arguments {
    // ID of the notebook
    #[clap(name = "id", env = "__FP_NOTEBOOK_ID")]
    id: String,

    #[clap(default_value_t = false, parse(from_flag), env = "__FP_SHELL_SESSION")]
    nested: bool,

    #[clap(from_global)]
    base_url: url::Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
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

struct MyTerminalState {
    alternate_mode: bool,
    buffer: String,
    current_line: String,
}

impl MyTerminalState {
    pub fn new() -> Self {
        Self {
            alternate_mode: false,
            buffer: String::new(),
            current_line: String::new(),
        }
    }

    fn flush(&mut self) {
        self.buffer
            .insert_str(self.buffer.len(), &self.current_line);
        self.current_line.clear();
        self.current_line.push('\n');
    }

    pub fn proccess(&mut self, action: Action) {
        match action {
            Action::Print(c) => {
                if !self.alternate_mode {
                    self.current_line.push(c);
                }
            }
            Action::Control(ControlCode::LineFeed) => {
                if !self.alternate_mode {
                    self.flush()
                }
            }
            Action::Control(ControlCode::Backspace) => {
                if !self.alternate_mode {
                    self.current_line.pop();
                }
            }
            //CSI(Mode(SetDecPrivateMode(Code(ClearAndEnableAlternateScreen))))
            Action::CSI(CSI::Mode(Mode::SetDecPrivateMode(DecPrivateMode::Code(code)))) => {
                match code {
                    DecPrivateModeCode::ClearAndEnableAlternateScreen => self.alternate_mode = true,
                    DecPrivateModeCode::EnableAlternateScreen => self.alternate_mode = true,
                    DecPrivateModeCode::OptEnableAlternateScreen => self.alternate_mode = true,
                    _ => {}
                }
            }
            //CSI(Mode(ResetDecPrivateMode(Code(ClearAndEnableAlternateScreen))))
            Action::CSI(CSI::Mode(Mode::ResetDecPrivateMode(DecPrivateMode::Code(code)))) => {
                match code {
                    DecPrivateModeCode::ClearAndEnableAlternateScreen => {
                        self.alternate_mode = false
                    }
                    DecPrivateModeCode::EnableAlternateScreen => self.alternate_mode = false,
                    DecPrivateModeCode::OptEnableAlternateScreen => self.alternate_mode = false,
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

#[instrument(err, skip_all)]
pub(crate) async fn handle_command(args: Arguments) -> Result<()> {
    let ts_format =
        time::format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]").unwrap();
    let shell_exe = std::env::var("SHELL").unwrap_or(DEFAULT_SHELL.to_string());

    let config = api_client_configuration(args.config, &args.base_url).await?;

    let user = get_profile(&config).await?;

    let header_cell = notebook_cells_append(
        &config,
        &args.id,
        vec![Cell::HeadingCell {
            id: String::new(),
            heading_type: HeadingType::H3,
            content: format!(
                "@{}'s shell session\n🟢 Started at:\t{}",
                user.name,
                time::OffsetDateTime::now_utc().format(&ts_format).unwrap()
            ),
            formatting: Some(vec![Annotation::MentionAnnotation {
                offset: 0,
                name: user.name,
                user_id: user.id,
            }]),
            read_only: Some(true),
        }],
    )
    .await?
    .pop()
    .ok_or_else(|| anyhow!("No cells returned"))?;

    let code_cell = notebook_cells_append(
        &config,
        &args.id,
        vec![Cell::CodeCell {
            id: String::new(),
            content: String::new(),
            read_only: Some(true),
            syntax: None,
        }],
    )
    .await?
    .pop()
    .ok_or_else(|| anyhow!("No cells returned"))?;

    let code_cell_id = match code_cell {
        Cell::CodeCell { id, .. } => id,
        _ => unreachable!(),
    };

    let header_id = match header_cell {
        Cell::HeadingCell { id, .. } => id,
        _ => unreachable!(),
    };

    let _raw = RawGuard::new();

    let mut in_buf = [0_u8; 4096];
    let mut out_buf = [0_u8; 4096];

    let mut stdin = Unblock::new(std::io::stdin());
    let mut stdout = tokio::io::stdout();

    let mut f = File::create("./log.txt").await?;

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

    let mut interval = tokio::time::interval(Duration::from_millis(250));

    let mut parser = termwiz::escape::parser::Parser::new();

    let mut my_terminal = MyTerminalState::new();

    loop {
        tokio::select! {
            biased;
            _ = &mut child_waiter => {
                break;
            },
            bytes = futures::AsyncReadExt::read(&mut stdin, &mut in_buf) => match bytes {
                Ok(bytes) => {
                    let data = &in_buf[..bytes];
                    writer.write_all(data).await.unwrap();
                }
                Err(e) => {
                    eprintln!("stdin read failed: {:?}", e);
                    break;
                }
            },
            bytes = reader.read(&mut out_buf) => match bytes {
                Ok(bytes) => {
                    let data = &out_buf[..bytes];
                    stdout.write_all(data).await.unwrap();
                    stdout.flush().await.unwrap();

                    for a in parser.parse_as_vec(data) {
                        f.write_all(format!("{:?}\n", a).as_bytes()).await?;

                        my_terminal.proccess(a);
                    }
                }
                Err(e) => {
                    eprintln!("pty read failed: {:?}", e);
                    break;
                }
            },
            _ = interval.tick() => {

                let buffer = &mut my_terminal.buffer;

                if buffer.is_empty() {
                    continue;
                }

                notebook_cell_append_text(&config, &args.id, &code_cell_id, CellAppendText { content: buffer.clone(), formatting: None }).await.unwrap();

                buffer.clear();
            },
            else => break,
        }
    }

    my_terminal.flush();

    let (a, b) = tokio::join!(
        notebook_cell_append_text(
            &config,
            &args.id,
            &code_cell_id,
            CellAppendText {
                content: my_terminal.buffer,
                formatting: None
            }
        ),
        notebook_cell_append_text(
            &config,
            &args.id,
            &header_id,
            CellAppendText {
                content: format!(
                    "\n🔴 Ended at:\t{}",
                    OffsetDateTime::now_utc().format(&ts_format).unwrap()
                ),
                formatting: None,
            },
        )
    );

    a.unwrap();
    b.unwrap();

    Ok(())
}
