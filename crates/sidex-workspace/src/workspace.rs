//! Workspace model — root path, file tree, watcher, recent files.

use std::path::{Path, PathBuf};

use crate::file_tree::FileTree;

const MAX_RECENT: usize = 50;

/// A workspace rooted at a directory.
pub struct Workspace {
    root: PathBuf,
    file_tree: FileTree,
    recent_files: Vec<PathBuf>,
}

impl Workspace {
    /// Open a workspace at `root`, scanning its file tree.
    pub fn open(root: &Path) -> Self {
        let file_tree = FileTree::scan(root);
        Self {
            root: root.to_path_buf(),
            file_tree,
            recent_files: Vec::new(),
        }
    }

    /// The root path of the workspace.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The current file tree.
    pub fn file_tree(&self) -> &FileTree {
        &self.file_tree
    }

    /// Mutable access for expanding / refreshing subtrees.
    pub fn file_tree_mut(&mut self) -> &mut FileTree {
        &mut self.file_tree
    }

    /// Recently opened files (most recent first).
    pub fn recent_files(&self) -> &[PathBuf] {
        &self.recent_files
    }

    /// Record a file as recently opened. Moves it to the front if already present.
    pub fn add_recent(&mut self, path: &Path) {
        let pb = path.to_path_buf();
        self.recent_files.retain(|p| p != &pb);
        self.recent_files.insert(0, pb);
        self.recent_files.truncate(MAX_RECENT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn open_workspace() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "hi").unwrap();
        let ws = Workspace::open(tmp.path());
        assert_eq!(ws.root(), tmp.path());
        assert!(ws.file_tree().root.children.is_some());
    }

    #[test]
    fn recent_files_ordering() {
        let tmp = TempDir::new().unwrap();
        let mut ws = Workspace::open(tmp.path());

        let a = PathBuf::from("/a.rs");
        let b = PathBuf::from("/b.rs");

        ws.add_recent(&a);
        ws.add_recent(&b);
        assert_eq!(ws.recent_files()[0], b);

        ws.add_recent(&a);
        assert_eq!(ws.recent_files()[0], a);
        assert_eq!(ws.recent_files().len(), 2);
    }
}
