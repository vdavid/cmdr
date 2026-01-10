//! HTTP client for license server validation.
//!
//! Communicates with the license server to validate subscription status.

use serde::{Deserialize, Serialize};

/// License server URL (configured at compile time).
#[cfg(debug_assertions)]
const LICENSE_SERVER_URL: &str = "https://license.getcmdr.com"; // Use same URL in debug

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
}
