use std::{ffi::OsStr, path::PathBuf};
use sysinfo::{ProcessRefreshKind, RefreshKind};

#[derive(Debug, PartialEq, Eq)]
pub enum ShellType {
    PowerShell,
    Cmd,
    Bash,
    Sh,
    Zsh,
}

impl ShellType {
    /// Attempt to auto detect the shell `fp` was launched from by looking at
    /// the parent process executable's file name.
    /// One might be tempted to use the `$SHELL` env var instead, however
    /// that is generally set by the system and not by the shell itself.
    /// This means that if the user has `bash` as their default shell but
    /// at the moment is inside a `sh`, `zsh` or other shell the env var
    /// will still be `bash`.
    /// Some more information on guessing the current shell can be found here:
    /// https://man.archlinux.org/man/community/perl-shell-guess/Shell::Guess.3pm.en
    pub fn auto_detect() -> (Self, PathBuf) {
        let sys = sysinfo::System::new_with_specifics(
            RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
        );

        let path = sys
            .process(sysinfo::get_current_pid().unwrap())
            .and_then(|process| process.parent())
            .and_then(|pid| sys.process(pid))
            .and_then(|process| process.exe())
            .unwrap();

        let exe = path
            .file_stem()
            .and_then(OsStr::to_str)
            .map(str::to_lowercase);

        (
            match exe.as_deref() {
                Some("pwsh" | "powershell") => ShellType::PowerShell,
                Some("cmd") => ShellType::Cmd,
                Some("bash") => ShellType::Bash,
                Some("sh") => ShellType::Sh,
                Some("zsh") => ShellType::Zsh,
                Some(shell) => panic!("Unsupported shell type {}", shell),
                None => panic!("Must be launched from a shell parent"),
            },
            path.to_owned(),
        )
    }
}
