//! Windows installer: launch the NSIS / MSI installer and exit.
//!
//! On Windows we cannot replace the running `.exe`, so the convention is to
//! ship an MSI or NSIS installer alongside the app bundle. We hand control
//! to the installer with flags that make it silent by default, and point
//! it at the same install directory the running app lives in.

use std::path::Path;
use std::process::Command;

use tokio::task;

use crate::{UpdateError, UpdateResult};

pub(super) async fn install(artifact: &Path) -> UpdateResult<()> {
    let artifact = artifact.to_path_buf();
    task::spawn_blocking(move || install_blocking(&artifact))
        .await
        .map_err(|e| UpdateError::InstallFailed(format!("join error: {e}")))?
}

fn install_blocking(artifact: &Path) -> UpdateResult<()> {
    let ext = artifact
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    match ext.as_str() {
        "msi" => run_msi(artifact),
        "exe" => run_exe(artifact),
        other => Err(UpdateError::InstallFailed(format!(
            "unsupported Windows installer format: .{other}"
        ))),
    }
}

pub(super) fn relaunch(install_root: &Path) -> UpdateResult<()> {
    let target = if install_root.is_file() {
        install_root.to_path_buf()
    } else {
        std::env::current_exe().unwrap_or_else(|_| install_root.to_path_buf())
    };
    Command::new(target)
        .spawn()
        .map_err(|e| UpdateError::InstallFailed(format!("relaunch spawn: {e}")))?;
    Ok(())
}

fn run_msi(artifact: &Path) -> UpdateResult<()> {
    // `msiexec /i file.msi /qn /norestart` → silent install, no restart.
    Command::new("msiexec")
        .arg("/i")
        .arg(artifact)
        .args(["/qn", "/norestart", "/promptrestart"])
        .spawn()
        .map_err(|e| UpdateError::InstallFailed(format!("msiexec spawn: {e}")))?;
    Ok(())
}

fn run_exe(artifact: &Path) -> UpdateResult<()> {
    // NSIS silent flags; also pass `/UPDATE` so custom NSIS scripts can branch.
    Command::new(artifact)
        .args(["/S", "/UPDATE", "/NCRC"])
        .spawn()
        .map_err(|e| UpdateError::InstallFailed(format!("installer spawn: {e}")))?;
    Ok(())
}
