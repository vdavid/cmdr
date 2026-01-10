//! License key verification using Ed25519 signatures.

use crate::licensing::LicenseData;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use tauri_plugin_store::StoreExt;

// Ed25519 public key (32 bytes, hex-encoded).
// Generate this with: cd apps/license-server && pnpm run generate-keys
// Then copy the public key here.
//noinspection SpellCheckingInspection
const PUBLIC_KEY_HEX: &str = "c3b18e765fc5c74f9fb7f3a9869d14c6bdeda1f28ec85aa6182de78113930d26";

const STORE_KEY_LICENSE: &str = "license_key";

/// Information about the current license.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseInfo {
    pub email: String,
    pub transaction_id: String,
    pub issued_at: String,
}

/// Activate a license key. Returns the license info if valid.
pub fn activate_license(app: &tauri::AppHandle, license_key: &str) -> Result<LicenseInfo, String> {
    // Validate the license key
    let data = validate_license_key(license_key)?;

    // Store the license key
    let store = app
        .store("license.json")
        .map_err(|e| format!("Failed to open store: {}", e))?;

    store.set(STORE_KEY_LICENSE, serde_json::json!(license_key));

    Ok(LicenseInfo {
        email: data.email,
        transaction_id: data.transaction_id,
        issued_at: data.issued_at,
    })
}

/// Get stored license info, if any.
pub fn get_license_info(app: &tauri::AppHandle) -> Option<LicenseInfo> {
    let store = app.store("license.json").ok()?;
    let license_key = store.get(STORE_KEY_LICENSE)?.as_str()?.to_string();

    validate_license_key(&license_key).ok().map(|data| LicenseInfo {
        email: data.email,
        transaction_id: data.transaction_id,
        issued_at: data.issued_at,
    })
}

/// Validate a license key and extract the data.
fn validate_license_key(license_key: &str) -> Result<LicenseData, String> {
    validate_license_key_with_public_key(license_key, PUBLIC_KEY_HEX)
}

/// Validate a license key with a specific public key.
/// This is separated for testing purposes.
fn validate_license_key_with_public_key(license_key: &str, public_key_hex: &str) -> Result<LicenseData, String> {
    // License format: base64(payload).base64(signature)
    let parts: Vec<&str> = license_key.trim().split('.').collect();
    if parts.len() != 2 {
        return Err("Invalid license key format".to_string());
    }

    let payload_bytes = BASE64
        .decode(parts[0])
        .map_err(|_| "Invalid license key: bad payload encoding")?;

    let signature_bytes = BASE64
        .decode(parts[1])
        .map_err(|_| "Invalid license key: bad signature encoding")?;

    // Parse public key
    let public_key_bytes = hex_decode(public_key_hex).map_err(|_| "Internal error: invalid public key")?;

    let public_key = VerifyingKey::from_bytes(
        &public_key_bytes
            .try_into()
            .map_err(|_| "Internal error: invalid public key length")?,
    )
    .map_err(|_| "Internal error: invalid public key")?;

    // Parse signature
    let signature = Signature::from_slice(&signature_bytes).map_err(|_| "Invalid license key: bad signature")?;

    // Verify signature
    public_key
        .verify(&payload_bytes, &signature)
        .map_err(|_| "Invalid license key: signature verification failed")?;

    // Parse payload
    let data: LicenseData =
        serde_json::from_slice(&payload_bytes).map_err(|_| "Invalid license key: bad payload data")?;

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
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid license key format"));
    }

    #[test]
    fn test_validate_license_key_invalid_format_multiple_dots() {
        let result = validate_license_key("too.many.dots");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid license key format"));
    }

    #[test]
    fn test_validate_license_key_bad_base64_payload() {
        let result = validate_license_key("not_valid_base64!!!.YWJj");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bad payload encoding"));
    }

    #[test]
    fn test_validate_license_key_bad_base64_signature() {
        // Valid base64 payload, invalid base64 signature
        let result = validate_license_key("YWJj.not_valid_base64!!!");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bad signature encoding"));
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
        let license_data = crate::licensing::LicenseData {
            email: "test@example.com".to_string(),
            transaction_id: "txn_test_123".to_string(),
            issued_at: "2026-01-08T12:00:00Z".to_string(),
            license_type: None,
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
        let original_data = crate::licensing::LicenseData {
            email: "original@example.com".to_string(),
            transaction_id: "txn_original".to_string(),
            issued_at: "2026-01-08T12:00:00Z".to_string(),
            license_type: None,
        };
        let original_json = serde_json::to_string(&original_data).unwrap();
        let signature = signing_key.sign(original_json.as_bytes());
        let signature_base64 = BASE64.encode(signature.to_bytes());

        // Create tampered payload (different email)
        let tampered_data = crate::licensing::LicenseData {
            email: "hacker@evil.com".to_string(),
            transaction_id: "txn_original".to_string(),
            issued_at: "2026-01-08T12:00:00Z".to_string(),
            license_type: None,
        };
        let tampered_json = serde_json::to_string(&tampered_data).unwrap();
        let tampered_payload_base64 = BASE64.encode(tampered_json.as_bytes());

        // Try to use original signature with tampered payload
        let tampered_license_key = format!("{}.{}", tampered_payload_base64, signature_base64);

        let result = validate_license_key_with_public_key(&tampered_license_key, &public_key_hex);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("signature verification failed"));
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
        let license_data = crate::licensing::LicenseData {
            email: "test@example.com".to_string(),
            transaction_id: "txn_test".to_string(),
            issued_at: "2026-01-08T12:00:00Z".to_string(),
            license_type: None,
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
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("signature verification failed"));
    }
}
