//! Shared helpers for running git commands via `std::process::Command`.

use std::path::Path;
use std::process::Command;

use crate::error::{GitError, GitResult};

/// Build a `Command` for git, with `CREATE_NO_WINDOW` on Windows.
pub(crate) fn git_command() -> Command {
    let cmd = Command::new("git");
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = cmd;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        cmd
    }
    #[cfg(not(target_os = "windows"))]
    cmd
}

/// Run a git command in `repo_root` and return stdout as a `String`.
pub(crate) fn run_git(repo_root: &Path, args: &[&str]) -> GitResult<String> {
    let output = git_command()
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(GitError::Exec)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::Command {
            message: stderr.trim().to_string(),
        });
    }

    String::from_utf8(output.stdout).map_err(GitError::Utf8)
}
