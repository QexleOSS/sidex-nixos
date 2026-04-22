//! Error types for the workspace crate.

use std::path::PathBuf;

/// Workspace error type.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("watcher error: {0}")]
    Watcher(#[from] notify::Error),

    #[error("regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("{0}")]
    Other(String),
}

/// Convenience alias.
pub type WorkspaceResult<T> = Result<T, WorkspaceError>;
