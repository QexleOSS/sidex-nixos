//! File icon theme support for the explorer and tabs.

use std::collections::HashMap;

/// Information about a resolved icon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconInfo {
    /// Path to the icon asset (relative to the theme root or a codicon id).
    pub icon_path: String,
    /// Optional tint color for the icon.
    pub color: Option<crate::Color>,
}

/// A file-icon theme that maps filenames, extensions, directories, and
/// language IDs to icons.
#[derive(Clone, Debug)]
pub struct FileIconTheme {
    pub name: String,
    file_names: HashMap<String, IconInfo>,
    file_extensions: HashMap<String, IconInfo>,
    folder_names: HashMap<String, IconInfo>,
    folder_names_expanded: HashMap<String, IconInfo>,
    language_ids: HashMap<String, IconInfo>,
    default_file: IconInfo,
    default_folder: IconInfo,
    default_folder_expanded: IconInfo,
    default_root_folder: IconInfo,
    default_root_folder_expanded: IconInfo,
}

impl Default for FileIconTheme {
    fn default() -> Self {
        Self::seti()
    }
}

impl FileIconTheme {
    /// Resolve an icon for a given filename.
    pub fn resolve_icon(
        &self,
        filename: &str,
        is_dir: bool,
        is_expanded: bool,
        language_id: Option<&str>,
    ) -> &IconInfo {
        if is_dir {
            return self.resolve_folder(filename, is_expanded);
        }
        if let Some(icon) = self.file_names.get(filename) {
            return icon;
        }
        if let Some(ext) = extension_of(filename) {
            if let Some(icon) = self.file_extensions.get(ext) {
                return icon;
            }
        }
        if let Some(lang) = language_id {
            if let Some(icon) = self.language_ids.get(lang) {
                return icon;
            }
        }
        &self.default_file
    }

    fn resolve_folder(&self, name: &str, expanded: bool) -> &IconInfo {
        if expanded {
            if let Some(icon) = self.folder_names_expanded.get(name) {
                return icon;
            }
            &self.default_folder_expanded
        } else {
            if let Some(icon) = self.folder_names.get(name) {
                return icon;
            }
            &self.default_folder
        }
    }

    /// The default root folder icon.
    pub fn root_folder_icon(&self, expanded: bool) -> &IconInfo {
        if expanded {
            &self.default_root_folder_expanded
        } else {
            &self.default_root_folder
        }
    }

    /// Built-in seti-like icon theme with common file extension mappings.
    #[allow(clippy::too_many_lines)]
    pub fn seti() -> Self {
        let mut exts = HashMap::new();
        let ext_map: &[(&str, &str)] = &[
            ("rs", "rust"),
            ("ts", "typescript"),
            ("tsx", "react_ts"),
            ("js", "javascript"),
            ("jsx", "react"),
            ("py", "python"),
            ("go", "go"),
            ("rb", "ruby"),
            ("java", "java"),
            ("c", "c"),
            ("cpp", "cpp"),
            ("h", "c"),
            ("hpp", "cpp"),
            ("cs", "csharp"),
            ("swift", "swift"),
            ("kt", "kotlin"),
            ("md", "markdown"),
            ("json", "json"),
            ("yaml", "yaml"),
            ("yml", "yaml"),
            ("toml", "toml"),
            ("xml", "xml"),
            ("html", "html"),
            ("css", "css"),
            ("scss", "sass"),
            ("less", "less"),
            ("svg", "svg"),
            ("png", "image"),
            ("jpg", "image"),
            ("gif", "image"),
            ("ico", "image"),
            ("webp", "image"),
            ("sh", "shell"),
            ("bash", "shell"),
            ("zsh", "shell"),
            ("fish", "shell"),
            ("ps1", "powershell"),
            ("sql", "database"),
            ("graphql", "graphql"),
            ("proto", "protobuf"),
            ("dockerfile", "docker"),
            ("lock", "lock"),
            ("log", "log"),
            ("txt", "text"),
            ("env", "settings"),
            ("gitignore", "git"),
            ("wasm", "wasm"),
            ("lua", "lua"),
            ("zig", "zig"),
            ("ex", "elixir"),
            ("exs", "elixir"),
            ("erl", "erlang"),
            ("hs", "haskell"),
            ("ml", "ocaml"),
            ("r", "r"),
            ("R", "r"),
            ("vue", "vue"),
            ("svelte", "svelte"),
            ("tf", "terraform"),
            ("dart", "dart"),
            ("php", "php"),
        ];
        for (ext, icon_name) in ext_map {
            exts.insert(
                (*ext).to_owned(),
                IconInfo {
                    icon_path: format!("seti/{icon_name}.svg"),
                    color: None,
                },
            );
        }

        let mut file_names = HashMap::new();
        let named_files: &[(&str, &str)] = &[
            ("Cargo.toml", "rust"),
            ("Cargo.lock", "lock"),
            ("package.json", "nodejs"),
            ("tsconfig.json", "typescript"),
            ("Makefile", "makefile"),
            ("CMakeLists.txt", "cmake"),
            ("Dockerfile", "docker"),
            ("docker-compose.yml", "docker"),
            (".gitignore", "git"),
            (".gitattributes", "git"),
            ("LICENSE", "license"),
            ("README.md", "readme"),
        ];
        for (name, icon_name) in named_files {
            file_names.insert(
                (*name).to_owned(),
                IconInfo {
                    icon_path: format!("seti/{icon_name}.svg"),
                    color: None,
                },
            );
        }

        let mut folder_names = HashMap::new();
        let mut folder_names_expanded = HashMap::new();
        let named_folders: &[(&str, &str)] = &[
            ("src", "folder-src"),
            ("lib", "folder-lib"),
            ("test", "folder-test"),
            ("tests", "folder-test"),
            ("node_modules", "folder-node"),
            (".git", "folder-git"),
            ("build", "folder-dist"),
            ("dist", "folder-dist"),
            ("target", "folder-dist"),
            ("docs", "folder-docs"),
            ("assets", "folder-images"),
            ("images", "folder-images"),
            (".vscode", "folder-vscode"),
            (".github", "folder-github"),
            ("config", "folder-config"),
        ];
        for (name, icon_name) in named_folders {
            folder_names.insert(
                (*name).to_owned(),
                IconInfo {
                    icon_path: format!("seti/{icon_name}.svg"),
                    color: None,
                },
            );
            folder_names_expanded.insert(
                (*name).to_owned(),
                IconInfo {
                    icon_path: format!("seti/{icon_name}-open.svg"),
                    color: None,
                },
            );
        }

        let lang_map: &[(&str, &str)] = &[
            ("rust", "rust"),
            ("typescript", "typescript"),
            ("javascript", "javascript"),
            ("python", "python"),
            ("go", "go"),
            ("markdown", "markdown"),
            ("json", "json"),
            ("html", "html"),
            ("css", "css"),
        ];
        let mut language_ids = HashMap::new();
        for (lang, icon_name) in lang_map {
            language_ids.insert(
                (*lang).to_owned(),
                IconInfo {
                    icon_path: format!("seti/{icon_name}.svg"),
                    color: None,
                },
            );
        }

        Self {
            name: "vs-seti".to_owned(),
            file_names,
            file_extensions: exts,
            folder_names,
            folder_names_expanded,
            language_ids,
            default_file: IconInfo {
                icon_path: "seti/default_file.svg".to_owned(),
                color: None,
            },
            default_folder: IconInfo {
                icon_path: "seti/default_folder.svg".to_owned(),
                color: None,
            },
            default_folder_expanded: IconInfo {
                icon_path: "seti/default_folder-open.svg".to_owned(),
                color: None,
            },
            default_root_folder: IconInfo {
                icon_path: "seti/default_root_folder.svg".to_owned(),
                color: None,
            },
            default_root_folder_expanded: IconInfo {
                icon_path: "seti/default_root_folder-open.svg".to_owned(),
                color: None,
            },
        }
    }
}

fn extension_of(filename: &str) -> Option<&str> {
    let name = filename.rsplit('/').next().unwrap_or(filename);
    let dot_pos = name.rfind('.')?;
    if dot_pos == 0 {
        return None;
    }
    Some(&name[dot_pos + 1..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_rust_file() {
        let theme = FileIconTheme::seti();
        let icon = theme.resolve_icon("main.rs", false, false, None);
        assert!(icon.icon_path.contains("rust"));
    }

    #[test]
    fn resolve_directory() {
        let theme = FileIconTheme::seti();
        let icon = theme.resolve_icon("src", true, false, None);
        assert!(icon.icon_path.contains("folder-src"));
    }

    #[test]
    fn resolve_expanded_directory() {
        let theme = FileIconTheme::seti();
        let icon = theme.resolve_icon("src", true, true, None);
        assert!(icon.icon_path.contains("open"));
    }

    #[test]
    fn resolve_by_language_id() {
        let theme = FileIconTheme::seti();
        let icon = theme.resolve_icon("unknown_file", false, false, Some("rust"));
        assert!(icon.icon_path.contains("rust"));
    }

    #[test]
    fn fallback_to_default() {
        let theme = FileIconTheme::seti();
        let icon = theme.resolve_icon("mystery", false, false, None);
        assert!(icon.icon_path.contains("default_file"));
    }

    #[test]
    fn named_file_match() {
        let theme = FileIconTheme::seti();
        let icon = theme.resolve_icon("Cargo.toml", false, false, None);
        assert!(icon.icon_path.contains("rust"));
    }
}
