//! Language server registry mapping language IDs to server configurations.
//!
//! Provides a [`ServerRegistry`] that knows how to launch the correct
//! language server for a given file type, with sensible built-in defaults
//! for popular languages.

use std::collections::HashMap;

use serde_json::Value;

/// Configuration for a language server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// The executable command (e.g. `"rust-analyzer"`).
    pub command: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Optional `initializationOptions` sent during `initialize`.
    pub initialization_options: Option<Value>,
    /// Optional workspace settings sent via `workspace/didChangeConfiguration`.
    pub settings: Option<Value>,
}

impl ServerConfig {
    /// Creates a new server configuration with the given command and arguments.
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            command: command.into(),
            args,
            initialization_options: None,
            settings: None,
        }
    }

    /// Sets `initializationOptions` for the server.
    #[must_use]
    pub fn with_initialization_options(mut self, options: Value) -> Self {
        self.initialization_options = Some(options);
        self
    }

    /// Sets workspace settings for the server.
    #[must_use]
    pub fn with_settings(mut self, settings: Value) -> Self {
        self.settings = Some(settings);
        self
    }
}

/// Maps language identifiers to their [`ServerConfig`].
#[derive(Debug, Clone)]
pub struct ServerRegistry {
    configs: HashMap<String, ServerConfig>,
}

impl Default for ServerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerRegistry {
    /// Creates a registry pre-populated with built-in defaults.
    pub fn new() -> Self {
        let mut configs = HashMap::new();

        configs.insert(
            "rust".to_owned(),
            ServerConfig::new("rust-analyzer", vec![]),
        );
        configs.insert(
            "typescript".to_owned(),
            ServerConfig::new("typescript-language-server", vec!["--stdio".to_owned()]),
        );
        configs.insert(
            "javascript".to_owned(),
            ServerConfig::new("typescript-language-server", vec!["--stdio".to_owned()]),
        );
        configs.insert("python".to_owned(), ServerConfig::new("pylsp", vec![]));
        configs.insert("go".to_owned(), ServerConfig::new("gopls", vec![]));
        configs.insert("c".to_owned(), ServerConfig::new("clangd", vec![]));
        configs.insert("cpp".to_owned(), ServerConfig::new("clangd", vec![]));

        Self { configs }
    }

    /// Creates an empty registry with no built-in defaults.
    pub fn empty() -> Self {
        Self {
            configs: HashMap::new(),
        }
    }

    /// Registers (or replaces) a server configuration for a language ID.
    pub fn register(&mut self, language_id: &str, config: ServerConfig) {
        self.configs.insert(language_id.to_owned(), config);
    }

    /// Looks up the server configuration for a language ID.
    pub fn get(&self, language_id: &str) -> Option<&ServerConfig> {
        self.configs.get(language_id)
    }

    /// Returns all registered language IDs.
    pub fn language_ids(&self) -> impl Iterator<Item = &str> {
        self.configs.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_rust() {
        let reg = ServerRegistry::new();
        let cfg = reg.get("rust").expect("rust should be registered");
        assert_eq!(cfg.command, "rust-analyzer");
        assert!(cfg.args.is_empty());
    }

    #[test]
    fn builtin_typescript() {
        let reg = ServerRegistry::new();
        let cfg = reg
            .get("typescript")
            .expect("typescript should be registered");
        assert_eq!(cfg.command, "typescript-language-server");
        assert_eq!(cfg.args, vec!["--stdio"]);
    }

    #[test]
    fn builtin_javascript_shares_ts_server() {
        let reg = ServerRegistry::new();
        let ts = reg.get("typescript").unwrap();
        let js = reg.get("javascript").unwrap();
        assert_eq!(ts.command, js.command);
    }

    #[test]
    fn builtin_python() {
        let reg = ServerRegistry::new();
        let cfg = reg.get("python").unwrap();
        assert_eq!(cfg.command, "pylsp");
    }

    #[test]
    fn builtin_go() {
        let reg = ServerRegistry::new();
        let cfg = reg.get("go").unwrap();
        assert_eq!(cfg.command, "gopls");
    }

    #[test]
    fn builtin_c_cpp() {
        let reg = ServerRegistry::new();
        assert_eq!(reg.get("c").unwrap().command, "clangd");
        assert_eq!(reg.get("cpp").unwrap().command, "clangd");
    }

    #[test]
    fn register_custom() {
        let mut reg = ServerRegistry::new();
        reg.register("zig", ServerConfig::new("zls", vec![]));
        let cfg = reg.get("zig").unwrap();
        assert_eq!(cfg.command, "zls");
    }

    #[test]
    fn register_overrides_builtin() {
        let mut reg = ServerRegistry::new();
        reg.register(
            "rust",
            ServerConfig::new("custom-ra", vec!["--flag".to_owned()]),
        );
        let cfg = reg.get("rust").unwrap();
        assert_eq!(cfg.command, "custom-ra");
    }

    #[test]
    fn get_missing_returns_none() {
        let reg = ServerRegistry::new();
        assert!(reg.get("cobol").is_none());
    }

    #[test]
    fn empty_registry() {
        let reg = ServerRegistry::empty();
        assert!(reg.get("rust").is_none());
    }

    #[test]
    fn with_initialization_options() {
        let cfg = ServerConfig::new("test", vec![])
            .with_initialization_options(serde_json::json!({"check": true}));
        assert!(cfg.initialization_options.is_some());
    }
}
