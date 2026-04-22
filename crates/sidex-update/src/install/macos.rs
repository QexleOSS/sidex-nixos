//! macOS installer: replace the `.app` bundle atomically with `ditto`.
//!
//! `SideX` ships as `SideX.app`. Tauri distributes updates as a `.tar.gz`
//! containing the replacement `.app`; we extract to a scratch dir, swap
//! the bundle with `ditto` (which preserves code-signing metadata and
//! extended attributes), then remove the scratch copy.

use std::path::{Path, PathBuf};
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
    let current_app = current_app_bundle()?;
    let staging =
        tempfile::tempdir().map_err(|e| UpdateError::InstallFailed(format!("staging dir: {e}")))?;

    extract_tar_gz(artifact, staging.path())?;
    let new_app = locate_app_bundle(staging.path())?;

    let status = Command::new("/usr/bin/ditto")
        .arg(&new_app)
        .arg(&current_app)
        .status()
        .map_err(|e| UpdateError::InstallFailed(format!("ditto spawn: {e}")))?;
    if !status.success() {
        return Err(UpdateError::InstallFailed(format!(
            "ditto returned non-zero exit status ({status})"
        )));
    }

    // Clear the quarantine bit so Gatekeeper doesn't warn on first launch.
    let _ = Command::new("/usr/bin/xattr")
        .args(["-rd", "com.apple.quarantine"])
        .arg(&current_app)
        .status();

    Ok(())
}

pub(super) fn relaunch(install_root: &Path) -> UpdateResult<()> {
    let bundle = if install_root.extension().and_then(|s| s.to_str()) == Some("app") {
        install_root.to_path_buf()
    } else {
        current_app_bundle().unwrap_or_else(|_| install_root.to_path_buf())
    };

    Command::new("/usr/bin/open")
        .arg("-n")
        .arg(bundle)
        .spawn()
        .map_err(|e| UpdateError::InstallFailed(format!("open -n spawn: {e}")))?;
    Ok(())
}

fn current_app_bundle() -> UpdateResult<PathBuf> {
    let exe = std::env::current_exe()?;
    // .../SideX.app/Contents/MacOS/SideX → .../SideX.app
    let mut path = exe;
    while let Some(parent) = path.parent() {
        if parent
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
        {
            return Ok(parent.to_path_buf());
        }
        path = parent.to_path_buf();
    }
    Err(UpdateError::InstallFailed(
        "could not locate running .app bundle".into(),
    ))
}

fn locate_app_bundle(root: &Path) -> UpdateResult<PathBuf> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry
            .path()
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
        {
            return Ok(entry.path());
        }
        // nested release archives sometimes put the app inside a subdir
        if entry.file_type()?.is_dir() {
            if let Ok(nested) = locate_app_bundle(&entry.path()) {
                return Ok(nested);
            }
        }
    }
    Err(UpdateError::InstallFailed(
        "no .app bundle found in archive".into(),
    ))
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> UpdateResult<()> {
    let status = Command::new("/usr/bin/tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(dest)
        .status()
        .map_err(|e| UpdateError::InstallFailed(format!("tar spawn: {e}")))?;
    if !status.success() {
        return Err(UpdateError::InstallFailed(format!(
            "tar returned non-zero exit status ({status})"
        )));
    }
    Ok(())
}
