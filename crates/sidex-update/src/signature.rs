//! Minisign signature verification (compatible with `tauri-plugin-updater`).
//!
//! The existing `SideX` release pipeline ships signatures in Minisign format
//! and embeds the public key in `tauri.conf.json`. Keeping byte-for-byte
//! compatibility means every release we've ever shipped continues to
//! verify after swapping the plugin for this crate.
//!
//! Minisign layout used by Tauri:
//!
//! ```text
//! untrusted comment: <arbitrary>
//! <base64 of (signature algorithm (2B) || key ID (8B) || payload))>
//! trusted comment: <arbitrary>
//! <base64 of global_sig (64B)>           // optional, not required for v1
//! ```
//!
//! For pure Ed25519 signatures (algorithm tag `Ed`) the payload is a 64-byte
//! detached signature over the file bytes.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::{UpdateError, UpdateResult};

/// Decodes a Minisign public key (the base64 blob stored in `tauri.conf.json`)
/// into an `ed25519-dalek` [`VerifyingKey`].
pub fn decode_public_key(pubkey_base64: &str) -> UpdateResult<VerifyingKey> {
    let bytes = STANDARD
        .decode(pubkey_base64.trim())
        .map_err(|e| UpdateError::SignatureInvalid(format!("pubkey base64: {e}")))?;
    // File contents: "untrusted comment: ...\n<base64>\n"; just the key bytes
    // after the newline are what we need. Accept either the raw file form
    // or a bare base64 key blob.
    let key_base64 = std::str::from_utf8(&bytes)
        .ok()
        .and_then(extract_last_nonempty_line)
        .unwrap_or(pubkey_base64.trim());
    let raw = STANDARD
        .decode(key_base64)
        .map_err(|e| UpdateError::SignatureInvalid(format!("pubkey payload: {e}")))?;
    if raw.len() != 42 {
        return Err(UpdateError::SignatureInvalid(format!(
            "expected 42-byte pubkey payload, got {}",
            raw.len()
        )));
    }
    if &raw[0..2] != b"Ed" {
        return Err(UpdateError::SignatureInvalid(
            "only Ed25519 (algo \"Ed\") keys are supported".into(),
        ));
    }
    let key_bytes: [u8; 32] = raw[10..42].try_into().expect("slice length checked above");
    VerifyingKey::from_bytes(&key_bytes)
        .map_err(|e| UpdateError::SignatureInvalid(format!("pubkey parse: {e}")))
}

/// Verifies a Minisign signature blob against a payload.
///
/// Mirrors the relaxed parser Tauri uses (both "raw base64" and the full
/// `untrusted comment: ...` form are accepted).
pub fn verify(pubkey: &VerifyingKey, signature_blob: &str, payload: &[u8]) -> UpdateResult<()> {
    let sig_base64 = extract_signature_base64(signature_blob)?;
    let raw = STANDARD
        .decode(sig_base64.as_bytes())
        .map_err(|e| UpdateError::SignatureInvalid(format!("sig base64: {e}")))?;
    if raw.len() < 74 {
        return Err(UpdateError::SignatureInvalid(format!(
            "sig payload too short ({} bytes)",
            raw.len()
        )));
    }
    if &raw[0..2] != b"Ed" {
        return Err(UpdateError::SignatureInvalid(
            "only Ed25519 (algo \"Ed\") signatures are supported".into(),
        ));
    }
    let sig_bytes: [u8; 64] = raw[10..74].try_into().expect("slice length checked above");
    let sig = Signature::from_bytes(&sig_bytes);
    pubkey
        .verify(payload, &sig)
        .map_err(|e| UpdateError::SignatureInvalid(format!("ed25519 verify: {e}")))
}

fn extract_signature_base64(blob: &str) -> UpdateResult<String> {
    for line in blob.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("untrusted comment:")
            || trimmed.starts_with("trusted comment:")
        {
            continue;
        }
        return Ok(trimmed.to_string());
    }
    Err(UpdateError::SignatureInvalid(
        "could not find signature payload line".into(),
    ))
}

fn extract_last_nonempty_line(raw: &str) -> Option<&str> {
    raw.lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty() && !l.starts_with("untrusted comment:"))
}
