//! License key verification using Ed25519 signatures.

use crate::licensing::LicenseData;
use crate::licensing::app_status;
use crate::licensing::redact_email;
use crate::licensing::validation_client::{activate_short_code, is_short_code};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri_plugin_store::StoreExt;

/// Typed errors for the license activation flow.
///
/// Serialized with `tag = "code"` so the frontend receives `{ code: "badSignature", ... }` and
/// can switch on the code instead of pattern-matching English error strings.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "code", rename_all = "camelCase")]
pub enum LicenseActivationError {
    /// Key doesn't have the expected format (no dot separator, wrong structure).
    InvalidFormat,
    /// Base64 decoding failed for payload or signature.
    BadEncoding,
    /// Ed25519 signature verification failed (typo, tampered key).
    BadSignature,
    /// JSON payload couldn't be parsed.
    BadPayload,
    /// Short code not found on the license server.
    ShortCodeNotFound,
    /// Couldn't reach the license server (network error, timeout).
    NetworkError { detail: String },
    /// License server returned an unexpected response.
    ServerError { detail: String },
}

impl std::fmt::Display for LicenseActivationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFormat => write!(f, "Invalid license key format"),
            Self::BadEncoding => write!(f, "Invalid license key: bad encoding"),
            Self::BadSignature => write!(f, "Invalid license key: signature verification failed"),
            Self::BadPayload => write!(f, "Invalid license key: bad payload data"),
            Self::ShortCodeNotFound => write!(f, "License code not found"),
            Self::NetworkError { detail } => write!(f, "Failed to connect to license server: {detail}"),
            Self::ServerError { detail } => write!(f, "Invalid response from license server: {detail}"),
        }
    }
}

impl std::error::Error for LicenseActivationError {}

/// In-memory cache for verified license info. Avoids re-parsing and re-verifying
/// the Ed25519 signature on every call to `get_license_info`.
static LICENSE_CACHE: Mutex<Option<LicenseInfo>> = Mutex::new(None);

// Ed25519 public key (32 bytes, hex-encoded).
// Generate this with: cd apps/license-server && pnpm run generate-keys
// Then copy the public key here.
//noinspection SpellCheckingInspection
const PUBLIC_KEY_HEX: &str = "c3b18e765fc5c74f9fb7f3a9869d14c6bdeda1f28ec85aa6182de78113930d26";

const STORE_KEY_LICENSE: &str = "license_key";
const STORE_KEY_SHORT_CODE: &str = "license_short_code";

/// Information about the current license.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseInfo {
    pub email: String,
    pub transaction_id: String,
    pub issued_at: String,
    pub organization_name: Option<String>,
    pub license_type: Option<String>,
    /// The short code used to activate (if available)
    pub short_code: Option<String>,
}

/// Result of verifying a license key without persisting it.
/// Kept separate from `LicenseInfo` so the full key doesn't leak to the frontend via `get_license_info`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResult {
    pub info: LicenseInfo,
    /// The full cryptographic key (same as input for full keys, exchanged for short codes).
    pub full_key: String,
    /// The original short code, if the input was a short code.
    pub short_code: Option<String>,
}

/// Verify a license key or short code without writing anything to disk or cache.
/// If the input is a short code, exchanges it for the full key via the server first.
/// This is the "try it" step — call `commit_license` afterwards to persist a verified key.
pub async fn verify_license_async(input: &str) -> Result<VerifyResult, LicenseActivationError> {
    let (full_key, short_code) = if is_short_code(input) {
        let key = activate_short_code(input).await?;
        (key, Some(input.to_string()))
    } else {
        (input.to_string(), None)
    };

    let data = validate_license_key(&full_key)?;
    // Prefer the input short code (user typed it), fall back to the one embedded in the payload
    let resolved_short_code = short_code.or(data.short_code);
    let info = LicenseInfo {
        email: data.email,
        transaction_id: data.transaction_id,
        issued_at: data.issued_at,
        organization_name: data.organization_name,
        license_type: data.license_type,
        short_code: resolved_short_code.clone(),
    };
    Ok(VerifyResult {
        info,
        full_key,
        short_code: resolved_short_code,
    })
}

/// Persist a verified license key to disk and update caches.
/// This is the "save it" step — only call after verification confirms the key is good
/// (or on network fallback when the key is crypto-valid but the server is unreachable).
pub fn commit_license(
    app: &tauri::AppHandle,
    license_key: &str,
    short_code: Option<&str>,
) -> Result<LicenseInfo, LicenseActivationError> {
    let data = validate_license_key(license_key)?;

    // Prefer the explicit short code (user typed it), fall back to the one embedded in the payload
    let resolved_short_code = short_code.map(|s| s.to_string()).or(data.short_code.clone());

    // Store the license key and optionally the short code
    let store = app
        .store("license.json")
        .map_err(|e| LicenseActivationError::ServerError {
            detail: format!("Failed to open store: {}", e),
        })?;

    store.set(STORE_KEY_LICENSE, serde_json::json!(license_key));
    if let Some(ref code) = resolved_short_code {
        store.set(STORE_KEY_SHORT_CODE, serde_json::json!(code));
    }

    // Write an initial cached status so get_app_status returns the correct license type
    // even before server validation. Deliberately does NOT write last_validation_timestamp,
    // so needs_validation() returns true and the frontend can detect "pending verification."
    let license_type = data
        .license_type
        .as_deref()
        .and_then(app_status::string_to_license_type);
    app_status::write_cached_status_without_validation(
        app,
        "active",
        license_type,
        data.organization_name.clone(),
        None,
    );

    let info = LicenseInfo {
        email: data.email,
        transaction_id: data.transaction_id,
        issued_at: data.issued_at,
        organization_name: data.organization_name,
        license_type: data.license_type,
        short_code: resolved_short_code,
    };

    // Update the in-memory cache with the newly committed license
    if let Ok(mut cache) = LICENSE_CACHE.lock() {
        *cache = Some(info.clone());
    }

    Ok(info)
}

/// Activate a license key (full key, not short code). Verifies + commits in one call.
/// Kept for backward compatibility with periodic validation and internal callers.
pub fn activate_license(app: &tauri::AppHandle, license_key: &str) -> Result<LicenseInfo, LicenseActivationError> {
    commit_license(app, license_key, None)
}

/// Activate a license key or short code (async version). Verifies + commits in one call.
/// If the input is a short code (CMDR-XXXX-XXXX-XXXX), it first exchanges it for the full key.
/// Kept for backward compatibility.
pub async fn activate_license_async(
    app: &tauri::AppHandle,
    input: &str,
) -> Result<LicenseInfo, LicenseActivationError> {
    let (full_key, short_code) = if is_short_code(input) {
        let key = activate_short_code(input).await?;
        (key, Some(input))
    } else {
        (input.to_string(), None)
    };

    commit_license(app, &full_key, short_code)
}

/// Get stored license info, if any. Returns a cached result after the first successful verification.
pub fn get_license_info(app: &tauri::AppHandle) -> Option<LicenseInfo> {
    // Fast path: return cached info if available
    if let Ok(cache) = LICENSE_CACHE.lock()
        && let Some(ref info) = *cache
    {
        return Some(info.clone());
    }

    // Slow path: read from store and verify Ed25519 signature
    let store = app.store("license.json").ok()?;
    let license_key = store.get(STORE_KEY_LICENSE)?.as_str()?.to_string();
    let short_code = store
        .get(STORE_KEY_SHORT_CODE)
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    let info = validate_license_key(&license_key).ok().map(|data| {
        // Prefer the stored short code, fall back to the one embedded in the payload
        let resolved_short_code = short_code.or(data.short_code);
        LicenseInfo {
            email: data.email,
            transaction_id: data.transaction_id,
            issued_at: data.issued_at,
            organization_name: data.organization_name,
            license_type: data.license_type,
            short_code: resolved_short_code,
        }
    })?;

    // Populate cache for subsequent calls
    if let Ok(mut cache) = LICENSE_CACHE.lock() {
        *cache = Some(info.clone());
    }

    Some(info)
}

/// Clear the in-memory license cache. Called when the license is reset.
pub fn clear_license_cache() {
    if let Ok(mut cache) = LICENSE_CACHE.lock() {
        *cache = None;
    }
}

/// Validate a license key and extract the data.
fn validate_license_key(license_key: &str) -> Result<LicenseData, LicenseActivationError> {
    validate_license_key_with_public_key(license_key, PUBLIC_KEY_HEX)
}

/// Validate a license key with a specific public key.
/// This is separated for testing purposes.
fn validate_license_key_with_public_key(
    license_key: &str,
    public_key_hex: &str,
) -> Result<LicenseData, LicenseActivationError> {
    // License format: base64(payload).base64(signature)
    let parts: Vec<&str> = license_key.trim().split('.').collect();
    if parts.len() != 2 {
        return Err(LicenseActivationError::InvalidFormat);
    }

    let payload_bytes = BASE64
        .decode(parts[0])
        .map_err(|_| LicenseActivationError::BadEncoding)?;

    let signature_bytes = BASE64
        .decode(parts[1])
        .map_err(|_| LicenseActivationError::BadEncoding)?;

    // Parse public key (internal error — should never happen with a valid compiled-in key)
    let public_key_bytes = hex_decode(public_key_hex).map_err(|_| LicenseActivationError::BadPayload)?;

    let public_key = VerifyingKey::from_bytes(
        &public_key_bytes
            .try_into()
            .map_err(|_| LicenseActivationError::BadPayload)?,
    )
    .map_err(|_| LicenseActivationError::BadPayload)?;

    // Parse signature
    let signature = Signature::from_slice(&signature_bytes).map_err(|_| LicenseActivationError::BadSignature)?;

    // Verify signature
    public_key
        .verify(&payload_bytes, &signature)
        .map_err(|_| LicenseActivationError::BadSignature)?;

    // Parse payload
    let data: LicenseData = serde_json::from_slice(&payload_bytes).map_err(|e| {
        log::info!("License payload parse error: {}", e);
        LicenseActivationError::BadPayload
    })?;

    log::debug!("License validated successfully for: {}", redact_email(&data.email));

    Ok(data)
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, ()> {
    if !hex.len().is_multiple_of(2) {
        return Err(());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_decode_valid() {
        let result = hex_decode("48656c6c6f").unwrap();
        assert_eq!(result, b"Hello");
    }

    #[test]
    fn test_hex_decode_empty() {
        let result = hex_decode("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_hex_decode_odd_length() {
        let result = hex_decode("abc");
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_decode_invalid_chars() {
        let result = hex_decode("gg");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_license_key_invalid_format_no_dot() {
        let result = validate_license_key("nodotinthiskey");
        assert!(matches!(result, Err(LicenseActivationError::InvalidFormat)));
    }

    #[test]
    fn test_validate_license_key_invalid_format_multiple_dots() {
        let result = validate_license_key("too.many.dots");
        assert!(matches!(result, Err(LicenseActivationError::InvalidFormat)));
    }

    #[test]
    fn test_validate_license_key_bad_base64_payload() {
        let result = validate_license_key("not_valid_base64!!!.YWJj");
        assert!(matches!(result, Err(LicenseActivationError::BadEncoding)));
    }

    #[test]
    fn test_validate_license_key_bad_base64_signature() {
        // Valid base64 payload, invalid base64 signature
        let result = validate_license_key("YWJj.not_valid_base64!!!");
        assert!(matches!(result, Err(LicenseActivationError::BadEncoding)));
    }

    /// Integration test: full cryptographic roundtrip
    /// This mimics what the license server does (sign) and what the app does (verify)
    #[test]
    fn test_full_cryptographic_roundtrip() {
        use ed25519_dalek::{Signer, SigningKey};
        use rand_core::OsRng;

        // Generate a test key pair
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        // Convert to hex for our functions
        let public_key_hex: String = verifying_key.as_bytes().iter().map(|b| format!("{:02x}", b)).collect();

        // Create license data (same structure as server)
        let license_data = LicenseData {
            email: "test@example.com".to_string(),
            transaction_id: "txn_test_123".to_string(),
            issued_at: "2026-01-08T12:00:00Z".to_string(),
            license_type: None,
            organization_name: Some("Test Corp".to_string()),
            short_code: None,
        };

        // Serialize payload (same as server)
        let payload_json = serde_json::to_string(&license_data).unwrap();
        let payload_bytes = payload_json.as_bytes();

        // Sign (same algorithm as server)
        let signature = signing_key.sign(payload_bytes);

        // Create license key in same format as server: base64(payload).base64(signature)
        let payload_base64 = BASE64.encode(payload_bytes);
        let signature_base64 = BASE64.encode(signature.to_bytes());
        let license_key = format!("{}.{}", payload_base64, signature_base64);

        // Validate using our Rust validation function
        let result = validate_license_key_with_public_key(&license_key, &public_key_hex);
        assert!(result.is_ok(), "Expected valid license but got: {:?}", result);

        let data = result.unwrap();
        assert_eq!(data.email, "test@example.com");
        assert_eq!(data.transaction_id, "txn_test_123");
        assert_eq!(data.issued_at, "2026-01-08T12:00:00Z");
        assert_eq!(data.organization_name, Some("Test Corp".to_string()));
    }

    /// Test that tampering with license key is detected
    #[test]
    fn test_tampered_license_key_rejected() {
        use ed25519_dalek::{Signer, SigningKey};
        use rand_core::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let public_key_hex: String = verifying_key.as_bytes().iter().map(|b| format!("{:02x}", b)).collect();

        // Create and sign original license
        let original_data = LicenseData {
            email: "original@example.com".to_string(),
            transaction_id: "txn_original".to_string(),
            issued_at: "2026-01-08T12:00:00Z".to_string(),
            license_type: None,
            organization_name: Some("Original Corp".to_string()),
            short_code: None,
        };
        let original_json = serde_json::to_string(&original_data).unwrap();
        let signature = signing_key.sign(original_json.as_bytes());
        let signature_base64 = BASE64.encode(signature.to_bytes());

        // Create tampered payload (different email)
        let tampered_data = LicenseData {
            email: "hacker@evil.com".to_string(),
            transaction_id: "txn_original".to_string(),
            issued_at: "2026-01-08T12:00:00Z".to_string(),
            license_type: None,
            organization_name: Some("Original Corp".to_string()),
            short_code: None,
        };
        let tampered_json = serde_json::to_string(&tampered_data).unwrap();
        let tampered_payload_base64 = BASE64.encode(tampered_json.as_bytes());

        // Try to use original signature with tampered payload
        let tampered_license_key = format!("{}.{}", tampered_payload_base64, signature_base64);

        let result = validate_license_key_with_public_key(&tampered_license_key, &public_key_hex);
        assert!(matches!(result, Err(LicenseActivationError::BadSignature)));
    }

    /// Test that wrong public key rejects valid license
    #[test]
    fn test_wrong_public_key_rejects_license() {
        use ed25519_dalek::{Signer, SigningKey};
        use rand_core::OsRng;

        // Generate two different key pairs
        let signing_key = SigningKey::generate(&mut OsRng);
        let wrong_key = SigningKey::generate(&mut OsRng);
        let wrong_public_hex: String = wrong_key
            .verifying_key()
            .as_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();

        // Create license signed with first key
        let license_data = LicenseData {
            email: "test@example.com".to_string(),
            transaction_id: "txn_test".to_string(),
            issued_at: "2026-01-08T12:00:00Z".to_string(),
            license_type: None,
            organization_name: None,
            short_code: None,
        };
        let payload_json = serde_json::to_string(&license_data).unwrap();
        let signature = signing_key.sign(payload_json.as_bytes());
        let license_key = format!(
            "{}.{}",
            BASE64.encode(payload_json.as_bytes()),
            BASE64.encode(signature.to_bytes())
        );

        // Try to validate with wrong public key
        let result = validate_license_key_with_public_key(&license_key, &wrong_public_hex);
        assert!(matches!(result, Err(LicenseActivationError::BadSignature)));
    }

    #[test]
    fn test_license_activation_error_serialization() {
        // Tag-based serialization with "code" field
        let err = LicenseActivationError::InvalidFormat;
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"code\":\"invalidFormat\""), "JSON: {}", json);

        let err = LicenseActivationError::BadSignature;
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"code\":\"badSignature\""), "JSON: {}", json);

        let err = LicenseActivationError::NetworkError {
            detail: "timeout".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"code\":\"networkError\""), "JSON: {}", json);
        assert!(json.contains("\"detail\":\"timeout\""), "JSON: {}", json);

        let err = LicenseActivationError::ShortCodeNotFound;
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"code\":\"shortCodeNotFound\""), "JSON: {}", json);
    }

    #[test]
    fn test_license_activation_error_display() {
        assert_eq!(
            LicenseActivationError::InvalidFormat.to_string(),
            "Invalid license key format"
        );
        assert_eq!(
            LicenseActivationError::BadSignature.to_string(),
            "Invalid license key: signature verification failed"
        );
        assert_eq!(
            LicenseActivationError::ShortCodeNotFound.to_string(),
            "License code not found"
        );
        assert_eq!(
            LicenseActivationError::NetworkError {
                detail: "connection refused".to_string()
            }
            .to_string(),
            "Failed to connect to license server: connection refused"
        );
    }
}
