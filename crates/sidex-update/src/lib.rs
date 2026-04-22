//! Native update manager for `SideX`.
//!
//! Provides a full VS Code-parity update state machine that fetches a
//! release manifest, downloads the appropriate platform archive, verifies
//! its integrity (SHA-256) and authenticity (Ed25519 / Minisign), and then
//! applies it via platform-native install logic.
//!
//! The crate exposes a small, UI-agnostic [`UpdateManager`] — wiring into a
//! Tauri command layer lives in `src-tauri`. The lifecycle matches the
//! `IUpdateService` contract in the VS Code TypeScript frontend, so the
//! state progression can be forwarded verbatim to the webview as events.
//!
//! # State machine
//!
//! ```text
//!   Uninitialized
//!         ↓
//!        Idle
//!       ↓   ↑
//!   CheckingForUpdates  →  AvailableForDownload
//!         ↓                        ↓
//!                         ←   Overwriting
//!      Downloading                ↑
//!                         →     Ready
//!         ↓                       ↑
//!      Downloaded        →     Updating
//! ```

pub mod download;
pub mod install;
pub mod manager;
pub mod manifest;
pub mod signature;
pub mod state;

pub use manager::{UpdateConfig, UpdateManager, UpdateObserver};
pub use manifest::{Platform, ReleaseManifest, UpdateInfo};
pub use state::{DisablementReason, State, StateType, UpdateType};

/// Errors produced by the update subsystem.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("update feed returned no release for the current platform")]
    NoReleaseForPlatform,

    #[error("update manifest is malformed: {0}")]
    MalformedManifest(String),

    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("integrity check failed: expected {expected}, computed {actual}")]
    IntegrityMismatch { expected: String, actual: String },

    #[error("signature verification failed: {0}")]
    SignatureInvalid(String),

    #[error("installer failed: {0}")]
    InstallFailed(String),

    #[error("operation not valid in current state ({0:?})")]
    InvalidState(StateType),

    #[error("operation cancelled")]
    Cancelled,

    #[error("this build is not configured for updates")]
    NotConfigured,
}

pub type UpdateResult<T> = std::result::Result<T, UpdateError>;
