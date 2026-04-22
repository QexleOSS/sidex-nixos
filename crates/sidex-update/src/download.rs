//! Streaming downloader with integrity verification and cancellation.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::{UpdateError, UpdateResult};

/// Observers a download can use to surface progress back up to the UI.
pub trait DownloadObserver: Send + Sync {
    fn on_progress(&self, downloaded: u64, total: Option<u64>);
}

/// Describes where the downloaded bundle should land and how to verify it.
pub struct DownloadJob<'a> {
    pub url: &'a str,
    pub destination: &'a Path,
    pub expected_sha256: Option<&'a str>,
    pub cancel: Arc<AtomicBool>,
}

/// Fully downloads the bundle at `job.url` to `job.destination`, streaming
/// bytes to disk so large installers never live entirely in memory.
///
/// Returns the resolved on-disk path (identical to `destination`). Exists as
/// a separate helper because the `Ready` state stores the path.
pub async fn download(
    client: &reqwest::Client,
    job: &DownloadJob<'_>,
    observer: &dyn DownloadObserver,
) -> UpdateResult<PathBuf> {
    if let Some(parent) = job.destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let response = client.get(job.url).send().await?.error_for_status()?;
    let total = response.content_length();

    let mut file = File::create(job.destination).await?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        if job.cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = tokio::fs::remove_file(&job.destination).await;
            return Err(UpdateError::Cancelled);
        }

        let bytes = chunk?;
        hasher.update(&bytes);
        file.write_all(&bytes).await?;
        downloaded = downloaded.saturating_add(bytes.len() as u64);
        observer.on_progress(downloaded, total);
    }

    file.flush().await?;
    file.sync_all().await?;
    drop(file);

    if let Some(expected) = job.expected_sha256 {
        let actual = hex::encode(hasher.finalize());
        if !expected.eq_ignore_ascii_case(&actual) {
            let _ = tokio::fs::remove_file(&job.destination).await;
            return Err(UpdateError::IntegrityMismatch {
                expected: expected.to_ascii_lowercase(),
                actual,
            });
        }
    }

    Ok(job.destination.to_path_buf())
}
