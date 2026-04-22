//! Error types for the git crate.

use std::path::PathBuf;

/// Git error type.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git command failed: {message}")]
    Command { message: String },

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to execute git: {0}")]
    Exec(#[from] std::io::Error),

    #[error("git output not valid UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("parse error: {0}")]
    Parse(String),
}

/// Convenience alias.
pub type GitResult<T> = Result<T, GitError>;
