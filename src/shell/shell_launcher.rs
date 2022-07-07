use super::shell_type::ShellType;
use super::terminal_extractor::{
    END_PROMPT_BYTES, END_PROMPT_CHAR, END_PROMPT_REPEATS, START_PROMPT_BYTES, START_PROMPT_CHAR,
    START_PROMPT_REPEATS,
};
use anyhow::Result;
use crossterm::cursor::MoveTo;
use crossterm::terminal::{Clear, ClearType};
use portable_pty::CommandBuilder;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tracing::trace;

#[derive(Debug)]
pub struct ShellLauncher {
    shell_type: ShellType,
    path: PathBuf,
    notebook_id: String,
}

pub const NESTED_SHELL_SESSION_ENV_VAR_NAME: &str = "__FP_SHELL_SESSION";

impl ShellLauncher {
    pub fn new(notebook_id: String) -> Self {
        let (shell_type, path) = ShellType::auto_detect();
        Self {
            shell_type,
            path,
            notebook_id,
        }
    }

    pub fn build_command(&self) -> CommandBuilder {
        let mut cmd = CommandBuilder::new(&self.path);

        cmd.cwd(std::env::current_dir().unwrap());
        cmd.env("NOTEBOOK_ID", &self.notebook_id);
        cmd.env(NESTED_SHELL_SESSION_ENV_VAR_NAME, "1");

        if self.shell_type == ShellType::PowerShell {
            // Launch powershell with a custom command and don't exit (aka stay interactive) after completing it.
            // The reason for doing this rather than the approach taken for unix shells below is because powershell
            // doesn't have a nice equivalent of `history` that also removes the command from the command history file
            // but luckily for us the command provided to `-command` seemingly doesn't end up in any history #hack
            let cmd_string = format!(
                // This command assigns a new prompt and saves the old one inside a closure.
                // The closure returns a function block which prints out the START_PROMPT_BYTES before the prompt and END_PROMPT_BYTES after executing the saved prompt
                // The reason only the START/STOP_PROMPT_CHAR is formatted in and used in `ConvertFromUtf32` is because otherwise it would be printed as *this* command
                // is executed which in turn would be picked up by the terminal extractor and output a PromptStart.
                // That initial PromptStart is used to detect when the terminal is fully initialized but we don't want to have *this* command show up in the user's
                // terminal history our output
                "$function:prompt = & {{
                    $__last_prompt = $function:prompt;
                    $BP = [char]::ConvertFromUtf32({:#x});
                    $EP = [char]::ConvertFromUtf32({:#x});
                    {{
                        Write-Host \"{}\" -NoNewline
                        return \"$(&$script:__last_prompt){}\"
                    }}.GetNewClosure()
                }}",
                START_PROMPT_CHAR as u32,
                END_PROMPT_CHAR as u32,
                "$BP".repeat(START_PROMPT_REPEATS),
                "$EP".repeat(END_PROMPT_REPEATS)
            );

            trace!(?cmd_string, "starting powershell with -Command");
            cmd.args(&["-NoExit", "-Command", &cmd_string]);
        }

        cmd
    }

    pub async fn initialize_shell<W: futures::io::AsyncWriteExt + Unpin>(
        &self,
        stdin: &mut W,
    ) -> Result<()> {
        match self.shell_type {
            ShellType::Bash | ShellType::Sh | ShellType::Zsh => {
                //this produces the escaped string: "\xE2\x80\x8B\xE2\x80\x8B"
                let escaped_start_bytes = String::from_utf8(
                    START_PROMPT_BYTES
                        .iter()
                        .flat_map(|b| std::ascii::escape_default(*b))
                        .collect(),
                )
                .unwrap();

                let escaped_end_bytes = String::from_utf8(
                    END_PROMPT_BYTES
                        .iter()
                        .flat_map(|b| std::ascii::escape_default(*b))
                        .collect(),
                )
                .unwrap();

                // For unix shells we do more or less the same as for Powershell above but with the escaping done on the rust side.
                // A magician never reveals his tricks so the export command from the shell history so the user can't press arrow up to see it :^)
                stdin
                    .write_all(
                            format!("export PS1=\"$(printf '{escaped_start_bytes}')${{PS1}}$(printf '{escaped_end_bytes}')\";history -d $(history 1)\n").as_bytes(),
                    )
                    .await?;
            }
            ShellType::PowerShell => {
                // For whatever reason Powershell doesn't properly pickup where its current cursor position
                // is when launched under a PTY so we have to clear and reset the cursor position get make
                // sure everything is lined up correctly
                tokio::io::stdout()
                    .write_all(format!("{}{}", Clear(ClearType::All), MoveTo(0, 0)).as_bytes())
                    .await?;
            }
            ShellType::Cmd => {
                stdin
                    .write_all(
                    format!("SET __BP__={START_PROMPT_CHAR}\r\nSET __EP__={END_PROMPT_CHAR}\r\nprompt {}$p$g{}\r\n",
                            "%__BP__%".repeat(START_PROMPT_REPEATS),
                            "%__EP__%".repeat(END_PROMPT_REPEATS)
                        ).as_bytes()
                    )
                    .await?;
            }
        }
        Ok(())
    }
}
