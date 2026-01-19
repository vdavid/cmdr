//! HTTP client for license server validation.
//!
//! Communicates with the license server to validate subscription status.

use serde::{Deserialize, Serialize};

/// License server URL (configured at compile time).
#[cfg(debug_assertions)]
const LICENSE_SERVER_URL: &str = "http://localhost:8787";

#[cfg(not(debug_assertions))]
const LICENSE_SERVER_URL: &str = "https://license.getcmdr.com";

/// Response from the /validate endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResponse {
    pub status: String, // "active", "expired", "invalid"
    #[serde(rename = "type")]
    pub license_type: Option<String>, // "supporter", "commercial_subscription", "commercial_perpetual"
    #[serde(rename = "organizationName")]
    pub organization_name: Option<String>,
    #[serde(rename = "expiresAt")]
    pub expires_at: Option<String>,
}

/// Request body for the /validate endpoint.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidationRequest {
    transaction_id: String,
}

/// Response from the /activate endpoint.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivateResponse {
    pub license_key: Option<String>,
    /// Organization name from KV store (also embedded in license_key payload).
    #[allow(dead_code)]
    pub organization_name: Option<String>,
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
/// Returns the full key or an error message.
pub async fn activate_short_code(code: &str) -> Result<String, String> {
    // In mock mode, return a mock key
    #[cfg(debug_assertions)]
    if std::env::var("CMDR_MOCK_LICENSE").is_ok() {
        return Err("Mock mode: short code activation not available".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let url = format!("{}/activate", LICENSE_SERVER_URL);

    let response = client
        .post(&url)
        .json(&ActivateRequest {
            code: code.trim().to_uppercase(),
        })
        .send()
        .await
        .map_err(|e| format!("Failed to connect to license server: {}", e))?;

    let status = response.status();
    let body: ActivateResponse = response
        .json()
        .await
        .map_err(|e| format!("Invalid response from license server: {}", e))?;

    if !status.is_success() {
        return Err(body.error.unwrap_or_else(|| "License code not found".to_string()));
    }

    body.license_key.ok_or_else(|| "No license key in response".to_string())
}

/// Validate a license with the server.
///
/// Returns the validation response or None if the request failed.
pub async fn validate_with_server(transaction_id: &str) -> Option<ValidationResponse> {
    // In mock mode, skip server validation
    #[cfg(debug_assertions)]
    if std::env::var("CMDR_MOCK_LICENSE").is_ok() {
        return None;
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let url = format!("{}/validate", LICENSE_SERVER_URL);

    let response = client
        .post(&url)
        .json(&ValidationRequest {
            transaction_id: transaction_id.to_string(),
        })
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        log::warn!("License validation request failed: {}", response.status());
        return None;
    }

    response.json::<ValidationResponse>().await.ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_request_serialization() {
        let request = ValidationRequest {
            transaction_id: "txn_123".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"transactionId\":\"txn_123\""));
    }

    #[test]
    fn test_validation_response_deserialization() {
        let json = r#"{
            "status": "active",
            "type": "commercial_subscription",
            "organizationName": "Test Corp",
            "expiresAt": "2027-01-10T00:00:00Z"
        }"#;

        let response: ValidationResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "active");
        assert_eq!(response.license_type, Some("commercial_subscription".to_string()));
        assert_eq!(response.organization_name, Some("Test Corp".to_string()));
        assert_eq!(response.expires_at, Some("2027-01-10T00:00:00Z".to_string()));
    }

    #[test]
    fn test_validation_response_minimal() {
        let json = r#"{
            "status": "invalid",
            "type": null,
            "organizationName": null,
            "expiresAt": null
        }"#;

        let response: ValidationResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "invalid");
        assert_eq!(response.license_type, None);
        assert_eq!(response.organization_name, None);
        assert_eq!(response.expires_at, None);
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
