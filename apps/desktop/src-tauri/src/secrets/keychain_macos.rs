//! macOS Keychain backend via `security-framework`.

use super::{SecretStore, SecretStoreError};
use log::debug;
use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};
use std::sync::OnceLock;

/// Prod (unsuffixed) service name. Prod runs leave `CMDR_INSTANCE_ID` unset and land here so
/// existing Keychain entries keep working bit-for-bit.
const PROD_SERVICE_NAME: &str = "Cmdr";

/// Cache the resolved service name across the process. The env var only changes if the
/// process re-execs, so reading it once is the right shape. Tests use a separate pure helper
/// (`compute_service_name`) so the cache here doesn't leak between cases.
static SERVICE_NAME_CACHE: OnceLock<String> = OnceLock::new();

/// Pure helper for `service_name()`: compute the Keychain `SERVICE_NAME` from an instance ID
/// string. Empty / whitespace input is treated as unset (matching the wrapper-side
/// convention in `instance-id.js` where the env var either holds a non-empty value or is
/// absent). Unset → `"Cmdr"`; set → `"Cmdr-<instance>"`.
fn compute_service_name(instance_id: Option<&str>) -> String {
    match instance_id {
        Some(s) if !s.trim().is_empty() => format!("{PROD_SERVICE_NAME}-{}", s.trim()),
        _ => PROD_SERVICE_NAME.to_string(),
    }
}

/// Resolve the Keychain service name from `CMDR_INSTANCE_ID`, cached for the process
/// lifetime. Prod stays on `"Cmdr"`; any non-empty instance ID maps to `"Cmdr-<instance>"`
/// so dev / E2E / per-worktree runs never stomp on prod credentials.
fn service_name() -> &'static str {
    SERVICE_NAME_CACHE.get_or_init(|| {
        let instance = std::env::var("CMDR_INSTANCE_ID").ok();
        compute_service_name(instance.as_deref())
    })
}

/// Stores secrets in the macOS Keychain.
pub struct KeychainStore;

impl SecretStore for KeychainStore {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
        debug!("Keychain: setting secret for key: {}", key);
        set_generic_password(service_name(), key, value)
            .map_err(|e| SecretStoreError::Other(format!("Failed to save to Keychain: {}", e)))
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
        debug!("Keychain: getting secret for key: {}", key);
        get_generic_password(service_name(), key).map_err(|e| classify_security_error(key, e))
    }

    fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
        debug!("Keychain: deleting secret for key: {}", key);
        delete_generic_password(service_name(), key).map_err(|e| classify_security_error(key, e))
    }
}

/// Stable OSStatus codes from Apple's `SecBase.h`. Match on these instead of
/// the `security-framework` `Display` impl, which localizes on non-English
/// systems and could be reformatted in any minor crate bump.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
const ERR_SEC_AUTH_FAILED: i32 = -25293;
const ERR_SEC_USER_CANCELED: i32 = -128;
const ERR_SEC_INTERACTION_NOT_ALLOWED: i32 = -25308;

fn classify_security_error(key: &str, error: security_framework::base::Error) -> SecretStoreError {
    match error.code() {
        ERR_SEC_ITEM_NOT_FOUND => SecretStoreError::NotFound(format!("No secret found for key: {}", key)),
        ERR_SEC_AUTH_FAILED | ERR_SEC_USER_CANCELED | ERR_SEC_INTERACTION_NOT_ALLOWED => {
            SecretStoreError::AccessDenied(format!("{}", error))
        }
        _ => SecretStoreError::Other(format!("{}", error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_instance_yields_prod_service_name() {
        assert_eq!(compute_service_name(None), "Cmdr");
    }

    #[test]
    fn empty_instance_treated_as_unset() {
        // Mirrors instance-id.js: empty string is the "no instance" sentinel, never a
        // legitimate suffix. Without this branch a stray `CMDR_INSTANCE_ID=` would write to
        // `Cmdr-` and silently fork off prod credentials.
        assert_eq!(compute_service_name(Some("")), "Cmdr");
        assert_eq!(compute_service_name(Some("   ")), "Cmdr");
    }

    #[test]
    fn classify_security_error_uses_osstatus_not_message() {
        // Regression for the medium-severity audit finding: the classifier
        // used to substring-match the localized `Display` of the error, so
        // `errSecItemNotFound` on a non-English macOS came back as `Other`
        // and broke the "missing key → empty default" fallback in the AI
        // API-key + SMB credential flows.
        let not_found = security_framework::base::Error::from(ERR_SEC_ITEM_NOT_FOUND);
        assert!(matches!(
            classify_security_error("ai.apiKey.openai", not_found),
            SecretStoreError::NotFound(_)
        ));

        let auth_failed = security_framework::base::Error::from(ERR_SEC_AUTH_FAILED);
        assert!(matches!(
            classify_security_error("ai.apiKey.openai", auth_failed),
            SecretStoreError::AccessDenied(_)
        ));

        let user_canceled = security_framework::base::Error::from(ERR_SEC_USER_CANCELED);
        assert!(matches!(
            classify_security_error("ai.apiKey.openai", user_canceled),
            SecretStoreError::AccessDenied(_)
        ));

        // Unknown OSStatus must NOT collapse into `NotFound`/`AccessDenied`.
        let unknown = security_framework::base::Error::from(-99999);
        assert!(matches!(
            classify_security_error("ai.apiKey.openai", unknown),
            SecretStoreError::Other(_)
        ));
    }

    #[test]
    fn dev_instance_suffixes_service_name() {
        assert_eq!(compute_service_name(Some("dev")), "Cmdr-dev");
        assert_eq!(compute_service_name(Some("dev-foo")), "Cmdr-dev-foo");
    }

    #[test]
    fn e2e_instance_suffixes_with_full_id() {
        assert_eq!(
            compute_service_name(Some("e2e-nonmtp1-12345")),
            "Cmdr-e2e-nonmtp1-12345"
        );
        assert_eq!(compute_service_name(Some("e2e-mtp-99999")), "Cmdr-e2e-mtp-99999");
    }

    /// Document the once-cache contract: `service_name()` reads the env once via `OnceLock`,
    /// so the first call wins for the process lifetime. Tests use `compute_service_name`
    /// directly to avoid coupling test order to the cache.
    #[test]
    fn service_name_caches_first_resolution() {
        let first = service_name();
        let second = service_name();
        assert_eq!(
            first, second,
            "service_name() must return the cached value on repeat calls"
        );
        // Also assert the cache content is non-empty so a future refactor that returns "" by
        // accident is caught here, not in production.
        assert!(!first.is_empty());
    }
}
