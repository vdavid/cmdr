//! macOS Keychain backend via `security-framework`.

use super::{SecretStore, SecretStoreError};
use crate::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use log::debug;
use security_framework::item::{ItemClass, ItemSearchOptions, Limit, SearchResult};
use security_framework::os::macos::keychain::SecKeychain;
use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};
use std::ffi::CStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

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

/// The name Keychain Access shows this instance's items under.
pub(super) fn item_name() -> &'static str {
    service_name()
}

/// Stores secrets in the macOS Keychain.
pub struct KeychainStore;

impl SecretStore for KeychainStore {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
        debug!("Keychain: setting secret for key: {}", LogPrivate::host(key));
        set_generic_password(service_name(), key, value)
            .map_err(|e| SecretStoreError::Other(format!("Failed to save to Keychain: {}", e)))
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
        debug!("Keychain: getting secret for key: {}", LogPrivate::host(key));
        get_generic_password(service_name(), key).map_err(|e| classify_security_error(key, e))
    }

    fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
        debug!("Keychain: deleting secret for key: {}", LogPrivate::host(key));
        delete_generic_password(service_name(), key).map_err(|e| classify_security_error(key, e))
    }

    /// Every generic password under this instance's service. Reads attributes only, never the
    /// secret data, so it raises no Keychain consent dialog.
    fn keys(&self) -> Result<Vec<String>, SecretStoreError> {
        debug!("Keychain: listing keys");
        let found = ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(service_name())
            .load_attributes(true)
            .limit(Limit::All)
            .search();
        match found {
            Ok(results) => Ok(results
                .iter()
                .filter_map(SearchResult::simplify_dict)
                .filter_map(|attributes| attributes.get(ACCOUNT_ATTRIBUTE).cloned())
                .collect()),
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(Vec::new()),
            Err(e) => Err(classify_security_error(service_name(), e)),
        }
    }

    fn vouching(&self) -> &dyn SecretStore {
        &PinnedKeychainStore
    }
}

/// The Keychain for an item whose being there has to say Cmdr put it there: only the login
/// keychain, by the path the user database gives, with every prompt refused.
/// `DETAILS.md` § Vouching.
struct PinnedKeychainStore;

/// One no-prompt stretch at a time, since the switch is process-wide and dropping its lock turns
/// prompts back on: two overlapping stretches would end each other early.
static NO_PROMPT_STRETCH: Mutex<()> = Mutex::new(());

/// Runs `action` on the login keychain with prompts off. Another thread's Keychain call during this
/// stretch would fail rather than prompt, so keep `action` to one call.
fn with_login_keychain_no_prompt<T>(
    key: &str,
    action: impl FnOnce(&SecKeychain) -> security_framework::base::Result<T>,
) -> Result<T, SecretStoreError> {
    let path = login_keychain_path(&account_home_dir()?);
    let _one_at_a_time = NO_PROMPT_STRETCH.lock_ignore_poison();
    let _no_prompts = SecKeychain::disable_user_interaction().map_err(|e| classify_security_error(key, e))?;
    let keychain = SecKeychain::open(&path).map_err(|e| classify_security_error(key, e))?;
    action(&keychain).map_err(|e| classify_security_error(key, e))
}

impl SecretStore for PinnedKeychainStore {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
        debug!("Keychain: setting pinned secret for key: {}", LogPrivate::host(key));
        with_login_keychain_no_prompt(key, |keychain| {
            keychain.set_generic_password(service_name(), key, value)
        })
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
        debug!("Keychain: getting pinned secret for key: {}", LogPrivate::host(key));
        with_login_keychain_no_prompt(key, |keychain| {
            keychain
                .find_generic_password(service_name(), key)
                .map(|(password, _item)| password.as_ref().to_vec())
        })
    }

    fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
        debug!("Keychain: deleting pinned secret for key: {}", LogPrivate::host(key));
        with_login_keychain_no_prompt(key, |keychain| {
            let (_password, item) = keychain.find_generic_password(service_name(), key)?;
            item.delete();
            // `delete` answers nothing, so ask whether it's gone: an item that stayed must not
            // read as turned off.
            match keychain.find_generic_password(service_name(), key) {
                Ok(_) => Err(security_framework::base::Error::from(ERR_SEC_AUTH_FAILED)),
                Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
                Err(e) => Err(e),
            }
        })
    }

    fn keys(&self) -> Result<Vec<String>, SecretStoreError> {
        KeychainStore.keys()
    }

    fn vouching(&self) -> &dyn SecretStore {
        self
    }
}

/// The login keychain's file under `home`. ❌ Never `SecKeychainCopyDefault` or a bare search:
/// this store names one keychain.
fn login_keychain_path(home: &Path) -> PathBuf {
    home.join("Library/Keychains/login.keychain-db")
}

/// This account's home folder from the user database. ❌ Never `HOME` or `dirs::home_dir`: whoever
/// launches Cmdr picks those.
fn account_home_dir() -> Result<PathBuf, SecretStoreError> {
    let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut buffer = vec![0 as libc::c_char; 16 * 1024];
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: `passwd` and `found` are valid for writes, and `buffer` is valid for its stated
    // length and outlives every read of the strings `getpwuid_r` points into it below.
    let status = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            passwd.as_mut_ptr(),
            buffer.as_mut_ptr(),
            buffer.len(),
            &raw mut found,
        )
    };
    if status != 0 || found.is_null() {
        return Err(SecretStoreError::Other(
            "This account isn't in the user database".to_string(),
        ));
    }
    // SAFETY: a zero status with a non-null `found` means `getpwuid_r` filled `passwd`, whose
    // `pw_dir` is a NUL-terminated string inside `buffer`, still alive here.
    let home = unsafe { CStr::from_ptr((*found).pw_dir) };
    Ok(PathBuf::from(std::ffi::OsStr::from_bytes(home.to_bytes())))
}

/// `kSecAttrAccount`'s key in a simplified attribute dictionary: where a generic password keeps
/// the key we stored it under.
const ACCOUNT_ATTRIBUTE: &str = "acct";

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
    fn the_pinned_store_names_the_login_keychain_under_the_given_home() {
        assert_eq!(
            login_keychain_path(Path::new("/Users/someone")),
            Path::new("/Users/someone/Library/Keychains/login.keychain-db")
        );
    }

    #[test]
    fn the_account_home_is_an_absolute_path_from_the_user_database() {
        assert!(account_home_dir().expect("the test's account exists").is_absolute());
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
