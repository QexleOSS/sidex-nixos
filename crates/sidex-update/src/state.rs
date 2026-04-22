//! State machine mirroring VS Code's `IUpdateService` lifecycle.
//!
//! Every variant is serialized with the same shape the TypeScript
//! `platform/update/common/update.ts` types use (`{ type: "...", ... }`),
//! so the frontend can forward Rust-emitted events straight into the
//! existing `onStateChange` emitter.

use serde::{Deserialize, Serialize};

use crate::manifest::UpdateInfo;

/// Classification of the bundle format for this build. Matches VS Code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateType {
    Setup,
    Archive,
    Snap,
}

/// Reason the updater is disabled in the current environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DisablementReason {
    NotBuilt,
    DisabledByEnvironment,
    ManuallyDisabled,
    Policy,
    MissingConfiguration,
    InvalidConfiguration,
    RunningAsAdmin,
}

/// Discriminant for [`State`], useful when reporting from error paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateType {
    Uninitialized,
    Idle,
    Disabled,
    #[serde(rename = "checking for updates")]
    CheckingForUpdates,
    #[serde(rename = "available for download")]
    AvailableForDownload,
    Downloading,
    Downloaded,
    Updating,
    Ready,
    Overwriting,
    Restarting,
}

/// Full update state as shipped to the frontend via Tauri events.
///
/// Matches `State` in `src/vs/platform/update/common/update.ts` so
/// the TypeScript side can use the payload unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum State {
    Uninitialized,
    Idle {
        #[serde(rename = "updateType")]
        update_type: UpdateType,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "notAvailable")]
        not_available: Option<bool>,
    },
    Disabled {
        reason: DisablementReason,
    },
    #[serde(rename = "checking for updates")]
    CheckingForUpdates {
        explicit: bool,
    },
    #[serde(rename = "available for download")]
    AvailableForDownload {
        update: UpdateInfo,
        #[serde(skip_serializing_if = "Option::is_none", rename = "canInstall")]
        can_install: Option<bool>,
    },
    Downloading {
        #[serde(skip_serializing_if = "Option::is_none")]
        update: Option<UpdateInfo>,
        explicit: bool,
        overwrite: bool,
        #[serde(skip_serializing_if = "Option::is_none", rename = "downloadedBytes")]
        downloaded_bytes: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "totalBytes")]
        total_bytes: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "startTime")]
        start_time: Option<u64>,
    },
    Downloaded {
        update: UpdateInfo,
        explicit: bool,
        overwrite: bool,
    },
    Updating {
        update: UpdateInfo,
        #[serde(skip_serializing_if = "Option::is_none", rename = "currentProgress")]
        current_progress: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "maxProgress")]
        max_progress: Option<f64>,
        explicit: bool,
    },
    Ready {
        update: UpdateInfo,
        explicit: bool,
        overwrite: bool,
    },
    Overwriting {
        update: UpdateInfo,
        explicit: bool,
    },
    Restarting {
        update: UpdateInfo,
    },
}

impl State {
    pub fn kind(&self) -> StateType {
        match self {
            State::Uninitialized => StateType::Uninitialized,
            State::Idle { .. } => StateType::Idle,
            State::Disabled { .. } => StateType::Disabled,
            State::CheckingForUpdates { .. } => StateType::CheckingForUpdates,
            State::AvailableForDownload { .. } => StateType::AvailableForDownload,
            State::Downloading { .. } => StateType::Downloading,
            State::Downloaded { .. } => StateType::Downloaded,
            State::Updating { .. } => StateType::Updating,
            State::Ready { .. } => StateType::Ready,
            State::Overwriting { .. } => StateType::Overwriting,
            State::Restarting { .. } => StateType::Restarting,
        }
    }

    pub fn idle(update_type: UpdateType) -> Self {
        State::Idle {
            update_type,
            error: None,
            not_available: None,
        }
    }

    pub fn idle_with_error(update_type: UpdateType, error: impl Into<String>) -> Self {
        State::Idle {
            update_type,
            error: Some(error.into()),
            not_available: None,
        }
    }

    pub fn idle_not_available(update_type: UpdateType) -> Self {
        State::Idle {
            update_type,
            error: None,
            not_available: Some(true),
        }
    }
}
