//! Authentication primitives for `SideX`.
//!
//! Provides OS-keyring-backed secret storage with a `SQLite` fallback for
//! environments where the keyring is unavailable (headless CI, containers).
//! Built on `keyring` so each platform uses the best backend:
//!
//! * macOS     — Keychain
//! * Windows   — Credential Manager
//! * Linux/BSD — Secret Service (libsecret) / `KWallet`
//!
//! All values are opaque blobs; structured interpretation happens in the
//! TypeScript authentication service. We index an "all known keys" list in
//! `SQLite` so the `keys()` listing contract stays cheap.

pub mod storage;

pub use storage::{SecretStorage, StorageError};
