//! Profile registry persistence.
//!
//! Mirrors the storage format VS Code's `UserDataProfilesService` uses —
//! two JSON documents saved under the application user-data directory:
//!
//! * `profiles.json`           — array of `StoredUserDataProfile`
//! * `profile-associations.json` — `StoredProfileAssociations`
//!
//! The webview still holds `localStorage` as its synchronous source of
//! truth (VS Code's abstract base class is synchronous by contract).
//! This crate backs those writes with durable disk storage so profiles
//! survive cache flushes, browser-storage quota evictions, and multi-
//! window syncs initiated from other processes.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::fs;

pub const PROFILES_FILE: &str = "profiles.json";
pub const PROFILE_ASSOCIATIONS_FILE: &str = "profile-associations.json";

/// Storage root for the profile registry. Typically `<app-data>/UserData`.
#[derive(Debug, Clone)]
pub struct ProfileStorage {
    root: PathBuf,
}

impl ProfileStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub async fn ensure_root(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.root).await
    }

    pub fn profiles_path(&self) -> PathBuf {
        self.root.join(PROFILES_FILE)
    }

    pub fn associations_path(&self) -> PathBuf {
        self.root.join(PROFILE_ASSOCIATIONS_FILE)
    }

    pub async fn load_profiles(&self) -> Result<Vec<StoredUserDataProfile>> {
        read_json(&self.profiles_path()).await
    }

    pub async fn save_profiles(&self, profiles: &[StoredUserDataProfile]) -> Result<()> {
        write_json(&self.profiles_path(), profiles).await
    }

    pub async fn load_associations(&self) -> Result<StoredProfileAssociations> {
        read_json(&self.associations_path()).await
    }

    pub async fn save_associations(&self, value: &StoredProfileAssociations) -> Result<()> {
        write_json(&self.associations_path(), value).await
    }
}

/// Stored profile shape — aligned with the TypeScript side so payloads
/// round-trip without translation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StoredUserDataProfile {
    pub name: String,
    #[serde(default)]
    pub location: serde_json::Value,
    #[serde(
        default,
        rename = "useDefaultFlags",
        skip_serializing_if = "Option::is_none"
    )]
    pub use_default_flags: Option<serde_json::Value>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub short_name: Option<String>,
    #[serde(default)]
    pub extra: serde_json::Value,
}

/// Association map (`workspaceId` → `profileId`, `emptyWindows` → `profileId`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredProfileAssociations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspaces: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_windows: Option<serde_json::Map<String, serde_json::Value>>,
}

async fn read_json<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T> {
    match fs::read(path).await {
        Ok(bytes) if bytes.is_empty() => Ok(T::default()),
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(Into::into),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(err) => Err(err.into()),
    }
}

async fn write_json<T: serde::Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let data = serde_json::to_vec_pretty(value)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &data).await?;
    fs::rename(tmp, path).await?;
    Ok(())
}
