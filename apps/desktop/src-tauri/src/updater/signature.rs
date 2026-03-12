//! Minisign signature verification for update tarballs.
//!
//! Both the public key and per-release signatures are double-encoded:
//! base64(minisign-text-format). Decode the outer base64 first, then parse
//! with the minisign-verify crate. This matches Tauri's internal behavior.

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use minisign_verify::{PublicKey, Signature};

/// The minisign public key from `tauri.conf.json`, base64-encoded.
const PUBKEY_BASE64: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEEyNjAxRjM2QkIxNjhDMEEK\
     UldRS2pCYTdOaDlnb2lwTE5wZUNVZE5FTTBScXRrTWJPb1EvN3J2Q0FVcC9JNzJkSmphUTl3V3MK";

/// Verifies that `data` was signed with the hardcoded public key.
///
/// `signature_base64` is the base64-encoded minisign signature string from `latest.json`.
pub fn verify(data: &[u8], signature_base64: &str) -> Result<(), String> {
    let pubkey_text = decode_base64_text(PUBKEY_BASE64).map_err(|e| format!("Couldn't decode public key: {e}"))?;
    let sig_text = decode_base64_text(signature_base64).map_err(|e| format!("Couldn't decode signature: {e}"))?;

    let public_key = PublicKey::decode(&pubkey_text).map_err(|e| format!("Couldn't parse public key: {e}"))?;
    let signature = Signature::decode(&sig_text).map_err(|e| format!("Couldn't parse signature: {e}"))?;

    // allow_legacy=true matches Tauri's behavior
    public_key
        .verify(data, &signature, true)
        .map_err(|e| format!("Signature verification didn't pass: {e}"))
}

/// Decodes a base64 string into a UTF-8 string (the inner minisign text format).
fn decode_base64_text(b64: &str) -> Result<String, String> {
    let bytes = BASE64_STANDARD.decode(b64).map_err(|e| format!("base64 decode: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("UTF-8 decode: {e}"))
}
