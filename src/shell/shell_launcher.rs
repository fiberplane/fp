use super::shell_type::ShellType;
use anyhow::Result;
use portable_pty::CommandBuilder;
use std::path::PathBuf;

#[derive(Debug)]
pub struct ShellLauncher {
    shell_type: ShellType,
    path: PathBuf,
    notebook_id: String,
}

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
        cmd.env("__FP_NOTEBOOK_ID", &self.notebook_id);
        cmd.env("__FP_SHELL_SESSION", "1");

        if self.shell_type == ShellType::PowerShell {
            cmd.args(&[
                "-noexit",
                "-command",
                r#"$function:prompt = & { $__last_prompt = $function:prompt; $BP = [char]::ConvertFromUtf32(0x200B); $EP = [char]::ConvertFromUtf32(0x200E); { Write-Host "$BP$BP" -NoNewline; &$script:__last_prompt; return "$EP$EP" }.GetNewClosure() }"#
            ]);
        }

        cmd
    }

    pub async fn initialize_shell<W: futures::io::AsyncWriteExt + Unpin>(
        &self,
        stdin: &mut W,
    ) -> Result<()> {
        match self.shell_type {
            ShellType::Bash | ShellType::Sh | ShellType::Zsh => {
                stdin
                    .write_all(
                            "export PS1=\"$(printf '\\xE2\\x80\\x8B\\xE2\\x80\\x8B')${PS1}$(printf '\\xE2\\x80\\x8E\\xE2\\x80\\x8E')\";history -d $(history 1)\n"
                        .as_bytes(),
                    )
                    .await?;
            }
            _ => {}
        }
        Ok(())
    }
}
