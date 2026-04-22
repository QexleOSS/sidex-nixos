use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sidex_lsp::{LspClient, ServerConfig, ServerRegistry};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

pub struct LspState {
    registry: ServerRegistry,
    clients: Mutex<HashMap<u32, Arc<LspClient>>>,
    next_id: Mutex<u32>,
}

impl LspState {
    pub fn new() -> Self {
        Self {
            registry: ServerRegistry::new(),
            clients: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LspServerInfo {
    pub name: String,
    pub languages: Vec<String>,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LspNotificationEvent {
    pub server_id: u32,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LspStartResult {
    pub server_id: u32,
    pub capabilities: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspStartArgs {
    pub language_id: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub root_uri: String,
}

#[allow(clippy::unnecessary_wraps, clippy::needless_pass_by_value)]
#[tauri::command]
pub fn lsp_get_server_registry(
    state: State<'_, Arc<LspState>>,
) -> Result<Vec<LspServerInfo>, String> {
    let reg = &state.registry;
    let mut seen = HashMap::<String, usize>::new();
    let mut servers: Vec<LspServerInfo> = Vec::new();

    let mut langs: Vec<String> = reg.language_ids().map(String::from).collect();
    langs.sort();

    for lang in langs {
        let Some(cfg) = reg.get(&lang) else { continue };
        if let Some(&idx) = seen.get(&cfg.command) {
            servers[idx].languages.push(lang);
        } else {
            seen.insert(cfg.command.clone(), servers.len());
            servers.push(LspServerInfo {
                name: cfg.command.clone(),
                languages: vec![lang],
                command: cfg.command.clone(),
                args: cfg.args.clone(),
            });
        }
    }
    Ok(servers)
}

#[allow(clippy::unnecessary_wraps, clippy::needless_pass_by_value)]
#[tauri::command]
pub fn lsp_get_supported_languages(state: State<'_, Arc<LspState>>) -> Result<Vec<String>, String> {
    let mut langs: Vec<String> = state.registry.language_ids().map(String::from).collect();
    langs.sort();
    Ok(langs)
}

#[tauri::command]
pub async fn lsp_start_server(
    app: AppHandle,
    state: State<'_, Arc<LspState>>,
    args: LspStartArgs,
) -> Result<LspStartResult, String> {
    let (command, server_args): (String, Vec<String>) = if let Some(cmd) = args.command {
        (cmd, args.args.unwrap_or_default())
    } else if let Some(lang) = args.language_id.as_deref() {
        let cfg: &ServerConfig = state
            .registry
            .get(lang)
            .ok_or_else(|| format!("no language server registered for '{lang}'"))?;
        (cfg.command.clone(), cfg.args.clone())
    } else {
        return Err("either command or languageId must be provided".into());
    };

    let arg_refs: Vec<&str> = server_args.iter().map(String::as_str).collect();
    let mut client = LspClient::start(&command, &arg_refs, &args.root_uri)
        .await
        .map_err(|e| e.to_string())?;

    let id = {
        let mut next = state.next_id.lock().await;
        let id = *next;
        *next += 1;
        id
    };

    let app_for_notifications = app.clone();
    client.on_notification(move |method, params| {
        let _ = app_for_notifications.emit(
            "lsp-notification",
            LspNotificationEvent {
                server_id: id,
                method,
                params,
            },
        );
    });

    let capabilities = client
        .server_capabilities()
        .and_then(|caps| serde_json::to_value(caps.raw()).ok())
        .unwrap_or(Value::Null);

    state.clients.lock().await.insert(id, Arc::new(client));

    Ok(LspStartResult {
        server_id: id,
        capabilities,
    })
}

#[tauri::command]
pub async fn lsp_send_request(
    state: State<'_, Arc<LspState>>,
    server_id: u32,
    method: String,
    params: Option<Value>,
) -> Result<Value, String> {
    let client = {
        let clients = state.clients.lock().await;
        clients
            .get(&server_id)
            .cloned()
            .ok_or_else(|| format!("LSP server {server_id} not found"))?
    };

    client
        .raw_request(&method, params)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn lsp_stop_server(
    state: State<'_, Arc<LspState>>,
    server_id: u32,
) -> Result<(), String> {
    let client = {
        let mut clients = state.clients.lock().await;
        clients
            .remove(&server_id)
            .ok_or_else(|| format!("LSP server {server_id} not found"))?
    };

    if let Some(mut client) = Arc::into_inner(client) {
        client.shutdown().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn lsp_list_servers(state: State<'_, Arc<LspState>>) -> Result<Vec<u32>, String> {
    Ok(state.clients.lock().await.keys().copied().collect())
}
