//! Tauri command bindings for the native update manager.
//!
//! The TypeScript `IUpdateService` on the frontend invokes these commands
//! and listens to the `sidex://update/state-change` event to mirror
//! `onStateChange`.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use sidex_update::{State, UpdateConfig, UpdateManager, UpdateObserver, UpdateResult, UpdateType};
use tauri::{AppHandle, Emitter, Manager};

/// Tauri event name carrying the latest [`State`] payload.
pub const STATE_EVENT: &str = "sidex://update/state-change";

/// App-state wrapper so Tauri can hold a single [`UpdateManager`] instance.
pub struct UpdateManagerState {
    manager: OnceLock<UpdateManager>,
}

impl Default for UpdateManagerState {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateManagerState {
    pub const fn new() -> Self {
        Self {
            manager: OnceLock::new(),
        }
    }

    pub fn set(&self, manager: UpdateManager) {
        let _ = self.manager.set(manager);
    }

    pub fn get(&self) -> Option<&UpdateManager> {
        self.manager.get()
    }
}

struct EventEmitter {
    app: AppHandle,
}

impl UpdateObserver for EventEmitter {
    fn on_state_change(&self, state: &State) {
        if let Err(err) = self.app.emit(STATE_EVENT, state) {
            log::warn!("failed to emit update state event: {err}");
        }
    }
}

/// Initializes the [`UpdateManager`] during Tauri setup.
///
/// Pulls feed endpoints and the Minisign public key from the bundled
/// `tauri.conf.json` so existing release infrastructure keeps working.
pub fn initialize(app: &AppHandle) -> UpdateResult<()> {
    let config = read_config(app);
    let manager = UpdateManager::new(config)?;
    manager.set_observer(Arc::new(EventEmitter { app: app.clone() }));

    app.state::<UpdateManagerState>().set(manager);
    Ok(())
}

fn read_config(app: &AppHandle) -> UpdateConfig {
    let raw_pubkey = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|v| v.get("pubkey"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let endpoints = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|v| v.get("endpoints"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    UpdateConfig {
        endpoints,
        pubkey: raw_pubkey,
        current_version: app.package_info().version.to_string(),
        cache_dir: cache_dir(app),
        update_type: default_update_type(),
        user_agent: format!(
            "sidex/{} ({})",
            app.package_info().version,
            std::env::consts::OS
        ),
    }
}

fn cache_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_cache_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("updates")
}

const fn default_update_type() -> UpdateType {
    if cfg!(target_os = "windows") {
        UpdateType::Setup
    } else {
        UpdateType::Archive
    }
}

fn require_manager(state: &UpdateManagerState) -> Result<&UpdateManager, String> {
    state
        .get()
        .ok_or_else(|| "update manager not initialized".to_string())
}

#[tauri::command]
pub async fn update_check(
    state: tauri::State<'_, UpdateManagerState>,
    explicit: bool,
) -> Result<State, String> {
    let manager = require_manager(&state)?.clone();
    manager
        .check_for_updates(explicit)
        .await
        .map_err(|e| e.to_string())?;
    Ok(manager.state())
}

#[tauri::command]
pub async fn update_download(
    state: tauri::State<'_, UpdateManagerState>,
    explicit: bool,
) -> Result<State, String> {
    let manager = require_manager(&state)?.clone();
    manager
        .download_update(explicit)
        .await
        .map_err(|e| e.to_string())?;
    Ok(manager.state())
}

#[tauri::command]
pub async fn update_apply(state: tauri::State<'_, UpdateManagerState>) -> Result<State, String> {
    let manager = require_manager(&state)?.clone();
    manager.apply_update().await.map_err(|e| e.to_string())?;
    Ok(manager.state())
}

#[tauri::command]
pub async fn update_cancel(state: tauri::State<'_, UpdateManagerState>) -> Result<(), String> {
    require_manager(&state)?.cancel();
    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn update_state(state: tauri::State<'_, UpdateManagerState>) -> Result<State, String> {
    Ok(require_manager(&state)?.state())
}

#[tauri::command]
pub async fn update_cleanup(state: tauri::State<'_, UpdateManagerState>) -> Result<(), String> {
    let manager = require_manager(&state)?.clone();
    manager.cleanup_cache().await.map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn update_quit_and_install(app: AppHandle) -> Result<(), String> {
    let install_root = std::env::current_exe().map_err(|e| e.to_string())?;
    sidex_update::install::relaunch(&install_root).map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}
