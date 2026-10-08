//! HTTP client for API server license validation.
//!
//! Asks the API server for its signed verdict on a license (revoked, expired, renewed).

use crate::licensing::verification::{
    AnswerRejection, LicenseActivationError, SignedValidationAnswer, verify_validation_answer,
};
use rand::RngExt;
use serde::{Deserialize, Serialize};

/// License server URL (configured at compile time).
#[cfg(debug_assertions)]
const LICENSE_SERVER_URL: &str = "http://localhost:8787";

#[cfg(not(debug_assertions))]
const LICENSE_SERVER_URL: &str = "https://api.getcmdr.com";

/// Response from the /validate endpoint. The plain `status` / `type` / ... fields beside
/// `signedAnswer` serve older app versions; this one reads only what's signed.
#[derive(Debug, Clone, Deserialize)]
pub struct ValidationResponse {
    #[serde(rename = "signedAnswer")]
    pub signed_answer: Option<SignedAnswerWire>,
}

/// `payload` is base64 JSON; `signature` covers a fixed prefix plus the payload bytes.
#[derive(Debug, Clone, Deserialize)]
pub struct SignedAnswerWire {
    pub payload: String,
    pub signature: String,
}

/// Outcome of a server validation attempt.
#[derive(Debug)]
pub enum ValidationOutcome {
    /// Server returned a verified, definitive answer (active, expired, or invalid).
    Success(SignedValidationAnswer),
    /// License server couldn't reach Paddle (HTTP 502). Treat like a transient error.
    UpstreamError,
    /// Client couldn't reach the license server at all (network/timeout).
    NetworkError,
    /// Something answered, but not with our signature over this request. Could be a proxy, a
    /// squatter on a lapsed domain, or a replay; treated exactly like an unreachable server.
    Unverified,
}

/// Request body for the /validate endpoint.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidationRequest {
    transaction_id: String,
    /// Hashed device identifier for fair-use tracking. `None` if the platform UUID couldn't be
    /// read.
    device_id: Option<String>,
    /// Fresh per request; the server signs it back, so no captured answer can be replayed.
    nonce: String,
}

/// 16 random bytes as lowercase hex, the shape the server's `isValidNonce` accepts.
fn new_nonce() -> String {
    let bytes: [u8; 16] = rand::rng().random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Turn a parsed 200 body into the verified answer for this request, or say why not.
fn accept_answer(
    response: &ValidationResponse,
    transaction_id: &str,
    nonce: &str,
) -> Result<SignedValidationAnswer, AnswerRejection> {
    let signed = response.signed_answer.as_ref().ok_or(AnswerRejection::Unsigned)?;
    verify_validation_answer(&signed.payload, &signed.signature, transaction_id, nonce)
}

/// Response from the /activate endpoint.
///
/// `organizationName` is intentionally not declared (serde drops unknown fields and the
/// org name is read from the verified license payload, not this response).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivateResponse {
    pub license_key: Option<String>,
    #[allow(
        dead_code,
        reason = "Deserialized from API response; production errors map to the typed LicenseActivationError, but the field is exercised by deserialization tests"
    )]
    pub error: Option<String>,
}

/// Request body for the /activate endpoint.
#[derive(Debug, Clone, Serialize)]
struct ActivateRequest {
    code: String,
}

/// Check if a string looks like a short license code (CMDR-XXXX-XXXX-XXXX).
pub fn is_short_code(input: &str) -> bool {
    let trimmed = input.trim().to_uppercase();
    // Match CMDR-XXXX-XXXX-XXXX format
    if !trimmed.starts_with("CMDR-") {
        return false;
    }
    let parts: Vec<&str> = trimmed.split('-').collect();
    if parts.len() != 4 {
        return false;
    }
    // Check each segment after "CMDR" is 4 chars
    parts[1..]
        .iter()
        .all(|p| p.len() == 4 && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Exchange a short license code for the full cryptographic key.
///
/// Returns the full key or a typed activation error.
pub async fn activate_short_code(code: &str) -> Result<String, LicenseActivationError> {
    // In mock mode, return a mock key
    #[cfg(any(debug_assertions, feature = "playwright-e2e"))]
    if std::env::var("CMDR_MOCK_LICENSE").is_ok() {
        return Err(LicenseActivationError::NetworkError {
            detail: "Mock mode: short code activation not available".to_string(),
        });
    }

    // E2E builds never reach the license server: an E2E build is a release build, so this would be
    // production traffic. A run that needs a license state sets `CMDR_MOCK_LICENSE` instead.
    if cfg!(feature = "playwright-e2e") {
        return Err(LicenseActivationError::NetworkError {
            detail: "E2E build: short code activation not available".to_string(),
        });
    }

    let client = cmdr_http::client_builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| LicenseActivationError::NetworkError {
            detail: format!("Failed to create HTTP client: {}", e),
        })?;

    let url = format!("{}/activate", LICENSE_SERVER_URL);

    let response = client
        .post(&url)
        .json(&ActivateRequest {
            code: code.trim().to_uppercase(),
        })
        .send()
        .await
        .map_err(|e| LicenseActivationError::NetworkError { detail: e.to_string() })?;

    let status = response.status();
    let body: ActivateResponse = response
        .json()
        .await
        .map_err(|e| LicenseActivationError::ServerError { detail: e.to_string() })?;

    if !status.is_success() {
        return Err(LicenseActivationError::ShortCodeNotFound);
    }

    body.license_key.ok_or_else(|| LicenseActivationError::ServerError {
        detail: "No license key in response".to_string(),
    })
}

/// Validate a license with the server.
///
/// Returns a `ValidationOutcome` distinguishing between:
/// - `Success`: server gave a definitive answer (active, expired, or invalid)
/// - `UpstreamError`: license server couldn't reach Paddle (HTTP 502)
/// - `NetworkError`: client couldn't reach the license server at all
pub async fn validate_with_server(transaction_id: &str) -> ValidationOutcome {
    // In mock mode, skip server validation
    #[cfg(any(debug_assertions, feature = "playwright-e2e"))]
    if std::env::var("CMDR_MOCK_LICENSE").is_ok() {
        return ValidationOutcome::NetworkError;
    }

    // E2E builds never reach the license server (production traffic from a release build). A
    // `NetworkError` falls back to the cached status without overwriting it, like a real outage.
    if cfg!(feature = "playwright-e2e") {
        return ValidationOutcome::NetworkError;
    }

    let client = match cmdr_http::client_builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return ValidationOutcome::NetworkError,
    };

    let url = format!("{}/validate", LICENSE_SERVER_URL);
    let nonce = new_nonce();

    let response = match client
        .post(&url)
        .json(&ValidationRequest {
            transaction_id: transaction_id.to_string(),
            device_id: super::device_id::get_device_id(),
            nonce: nonce.clone(),
        })
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            // In debug builds the server is localhost:8787, which usually isn't running, so an
            // unreachable server is the normal dev state: info there, warn in release.
            if cfg!(debug_assertions) {
                log::info!("License validation network error: {}", e);
            } else {
                log::warn!("License validation network error: {}", e);
            }
            return ValidationOutcome::NetworkError;
        }
    };

    let status = response.status();

    // HTTP 502: license server couldn't reach Paddle
    if status.as_u16() == 502 {
        log::warn!("License server returned 502 (upstream Paddle error)");
        return ValidationOutcome::UpstreamError;
    }

    if !status.is_success() {
        log::warn!("License validation request failed: {}", status);
        return ValidationOutcome::NetworkError;
    }

    let parsed = match response.json::<ValidationResponse>().await {
        Ok(resp) => resp,
        Err(e) => {
            log::warn!("License validation response parse error: {}", e);
            return ValidationOutcome::NetworkError;
        }
    };

    match accept_answer(&parsed, transaction_id, &nonce) {
        Ok(answer) => ValidationOutcome::Success(answer),
        Err(rejection) => {
            log::warn!("License validation answer not trusted ({rejection:?}); keeping the cached status");
            ValidationOutcome::Unverified
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_request_serialization() {
        let request = ValidationRequest {
            transaction_id: "txn_123".to_string(),
            device_id: None,
            nonce: "0123456789abcdef0123456789abcdef".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"transactionId\":\"txn_123\""));
        assert!(json.contains("\"deviceId\":null"));
        assert!(json.contains("\"nonce\":\"0123456789abcdef0123456789abcdef\""));
    }

    #[test]
    fn test_validation_request_serialization_with_device_id() {
        let request = ValidationRequest {
            transaction_id: "txn_456".to_string(),
            device_id: Some("v1:abc123".to_string()),
            nonce: new_nonce(),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"transactionId\":\"txn_456\""));
        assert!(json.contains("\"deviceId\":\"v1:abc123\""));
    }

    #[test]
    fn a_nonce_is_fresh_lowercase_hex_the_server_accepts() {
        let nonce = new_nonce();

        assert_eq!(nonce.len(), 32);
        assert!(nonce.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(nonce, new_nonce());
    }

    #[test]
    fn reads_the_signed_answer_beside_the_legacy_fields() {
        let json = r#"{
            "status": "active",
            "type": "commercial_subscription",
            "organizationName": "Test Corp",
            "expiresAt": "2027-01-10T00:00:00Z",
            "signedAnswer": { "payload": "cGF5bG9hZA==", "signature": "c2ln" }
        }"#;

        let response: ValidationResponse = serde_json::from_str(json).unwrap();

        let signed = response.signed_answer.expect("signed answer present");
        assert_eq!(signed.payload, "cGF5bG9hZA==");
        assert_eq!(signed.signature, "c2ln");
    }

    #[test]
    fn an_unsigned_answer_is_not_trusted() {
        // What an older server, a proxy, or a squatter on a lapsed domain would send.
        let json = r#"{ "status": "invalid", "type": null, "organizationName": null, "expiresAt": null }"#;
        let response: ValidationResponse = serde_json::from_str(json).unwrap();

        let result = accept_answer(&response, "txn_1", "0123456789abcdef0123456789abcdef");

        assert!(matches!(result, Err(AnswerRejection::Unsigned)));
    }

    #[test]
    fn test_is_short_code_valid() {
        assert!(is_short_code("CMDR-ABCD-EFGH-1234"));
        assert!(is_short_code("cmdr-abcd-efgh-1234")); // Case insensitive
        assert!(is_short_code("  CMDR-ABCD-EFGH-1234  ")); // Whitespace trimmed
        assert!(is_short_code("CMDR-2345-6789-WXYZ"));
    }

    #[test]
    fn test_is_short_code_invalid() {
        assert!(!is_short_code("ABCD-EFGH-IJKL-MNOP")); // No CMDR prefix
        assert!(!is_short_code("CMDR-ABC-EFGH-1234")); // Segment too short
        assert!(!is_short_code("CMDR-ABCDE-FGHI-1234")); // Segment too long
        assert!(!is_short_code("CMDR-ABCD-EFGH")); // Missing segment
        assert!(!is_short_code("something.signature")); // Full key format
        assert!(!is_short_code("")); // Empty
        assert!(!is_short_code("CMDR")); // Just prefix
    }

    #[test]
    fn test_activate_response_success() {
        let json = r#"{
            "licenseKey": "eyJlbWFpbCI6InRlc3RAZXhhbXBsZS5jb20ifQ==.c2lnbmF0dXJl",
            "organizationName": "Acme Corp"
        }"#;

        let response: ActivateResponse = serde_json::from_str(json).unwrap();
        assert!(response.license_key.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_activate_response_error() {
        let json = r#"{
            "error": "License code not found or expired"
        }"#;

        let response: ActivateResponse = serde_json::from_str(json).unwrap();
        assert!(response.license_key.is_none());
        assert_eq!(response.error, Some("License code not found or expired".to_string()));
    }
}
