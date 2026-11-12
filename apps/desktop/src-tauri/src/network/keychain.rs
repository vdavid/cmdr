//! Keychain integration for SMB credentials.
//!
//! Delegates to `crate::secrets::store()` for platform-agnostic secret storage.
//! Credentials are cached in-memory after first access to avoid
//! repeated backend lookups during a session.

use crate::ignore_poison::RwLockIgnorePoison as _;
use crate::secrets::SecretStoreError;
use cmdr_fs::log_private::LogPrivate;
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// In-memory cache for credentials to avoid repeated backend lookups.
/// Key is the account name (like "smb://server" or "smb://server/share").
static CREDENTIAL_CACHE: std::sync::LazyLock<RwLock<HashMap<String, SmbCredentials>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// Credentials for SMB authentication.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SmbCredentials {
    /// Username for authentication
    pub username: String,
    /// Password for authentication
    pub password: String,
}

/// Error types for Keychain operations.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "type", content = "message")]
pub enum KeychainError {
    /// Credentials not found
    NotFound(String),
    /// Access denied (user cancelled or insufficient permissions)
    AccessDenied(String),
    /// Other error
    Other(String),
}

impl std::fmt::Display for KeychainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Credentials not found: {}", msg),
            Self::AccessDenied(msg) => write!(f, "Credential access denied: {}", msg),
            Self::Other(msg) => write!(f, "Credential error: {}", msg),
        }
    }
}

impl std::error::Error for KeychainError {}

impl KeychainError {
    /// The variant as a stable log token, next to the full message in `detail=`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::AccessDenied(_) => "access_denied",
            Self::Other(_) => "other",
        }
    }
}

impl From<SecretStoreError> for KeychainError {
    fn from(e: SecretStoreError) -> Self {
        match e {
            SecretStoreError::NotFound(msg) => KeychainError::NotFound(msg),
            SecretStoreError::AccessDenied(msg) => KeychainError::AccessDenied(msg),
            SecretStoreError::Other(msg) => KeychainError::Other(msg),
        }
    }
}

/// Creates the account name used for credential storage.
/// Format: "smb://{server}/{share}" or "smb://{server}" for server-level credentials.
///
/// The server is collapsed to its stable identity via
/// [`crate::network::server_identity::credential_key`] so that every name form of one
/// server (mDNS instance name, `.local` hostname, `statfs` service name) keys the same
/// entry. Without this, a password saved by the frontend under `Testnas` is invisible
/// to the upgrade path looking up `Testnas._smb._tcp.local`.
///
/// The share is NFC-folded for the same reason one step down: it reaches the frontend
/// composed (from the server's share list) and the upgrade path decomposed (from
/// `statfs`), so an accented share saved one way is looked up the other and silently
/// falls back to guest. `credential_key` already folds the server half.
///
/// ❗ The server half keeps a port off 445 (`smb://localhost:11482`), however the
/// caller spelled it: the sign-in sheet passes the discovery name, the upgrade passes
/// `server_identity::smb_server(host, port)`. On 445 the key is exactly the bare name.
/// The in-memory cache is keyed by this same account name, so it can't disagree.
fn make_account_name(server: &str, share: Option<&str>) -> String {
    use unicode_normalization::UnicodeNormalization;

    let key = crate::network::server_identity::credential_key(server);
    match share {
        Some(s) => format!("smb://{}/{}", key, s.nfc().collect::<String>()),
        None => format!("smb://{}", key),
    }
}

/// The account [`make_account_name`] gave `server` before its key dropped a trailing
/// dot ahead of the port (`server_identity::legacy_credential_key`), when that differs.
/// ❗ Forget only: nothing reads or writes there any more, but a password saved there
/// must not outlive its server.
fn legacy_account_name(server: &str, share: Option<&str>) -> Option<String> {
    use unicode_normalization::UnicodeNormalization;

    let key = crate::network::server_identity::legacy_credential_key(server)?;
    Some(match share {
        Some(s) => format!("smb://{}/{}", key, s.nfc().collect::<String>()),
        None => format!("smb://{}", key),
    })
}

/// Parses a stored password entry to extract username and password.
/// Format: "username\0password" (null-separated)
fn parse_password_entry(data: &[u8]) -> Option<SmbCredentials> {
    let text = String::from_utf8_lossy(data);
    let parts: Vec<&str> = text.splitn(2, '\0').collect();
    if parts.len() == 2 {
        Some(SmbCredentials {
            username: parts[0].to_string(),
            password: parts[1].to_string(),
        })
    } else {
        None
    }
}

/// Creates a password entry for storage.
/// Format: "username\0password" (null-separated)
fn make_password_entry(username: &str, password: &str) -> Vec<u8> {
    format!("{}\0{}", username, password).into_bytes()
}

/// Saves SMB credentials to the secret store.
pub fn save_credentials(
    server: &str,
    share: Option<&str>,
    username: &str,
    password: &str,
) -> Result<(), KeychainError> {
    let account = make_account_name(server, share);
    let entry = make_password_entry(username, password);

    debug!(
        "Saving credentials: server={server:?}, share={share:?}",
        server = LogPrivate::host(server),
        share = share.map(LogPrivate::volume)
    );

    crate::secrets::store().set(&account, &entry)?;

    // Update the in-memory cache
    CREDENTIAL_CACHE.write_ignore_poison().insert(
        account,
        SmbCredentials {
            username: username.to_string(),
            password: password.to_string(),
        },
    );

    Ok(())
}

/// Retrieves SMB credentials from the secret store.
pub fn get_credentials(server: &str, share: Option<&str>) -> Result<SmbCredentials, KeychainError> {
    let account = make_account_name(server, share);

    // Check in-memory cache first
    if let Some(creds) = CREDENTIAL_CACHE.read_ignore_poison().get(&account) {
        debug!(
            "Returning cached credentials: server={server:?}, share={share:?}",
            server = LogPrivate::host(server),
            share = share.map(LogPrivate::volume)
        );
        return Ok(creds.clone());
    }

    debug!(
        "Getting credentials: server={server:?}, share={share:?}",
        server = LogPrivate::host(server),
        share = share.map(LogPrivate::volume)
    );

    let data = crate::secrets::store().get(&account)?;
    let creds = parse_password_entry(&data)
        .ok_or_else(|| KeychainError::Other("Invalid credential format in store".to_string()))?;

    // Cache the credentials for future use
    CREDENTIAL_CACHE.write_ignore_poison().insert(account, creds.clone());

    Ok(creds)
}

/// Deletes SMB credentials from the secret store.
pub fn delete_credentials(server: &str, share: Option<&str>) -> Result<(), KeychainError> {
    let account = make_account_name(server, share);

    debug!(
        "Deleting credentials: server={server:?}, share={share:?}",
        server = LogPrivate::host(server),
        share = share.map(LogPrivate::volume)
    );

    // Remove from cache first
    CREDENTIAL_CACHE.write_ignore_poison().remove(&account);

    crate::secrets::store().delete(&account)?;

    Ok(())
}

/// Port-less entries a lookup found an off-445 server's password under this session,
/// keyed by that server's own key (`credential_key`, port and all).
///
/// ❗ Recorded so "Also forget the saved password" can take that entry too, and ONLY
/// that one: a port-less key is also the key of the server on 445 of the same
/// machine, so deleting one nobody found for this server would take another
/// server's password.
static FOUND_UNDER_PORTLESS: std::sync::LazyLock<RwLock<HashMap<String, String>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// Notes that `server`'s (`host:port`) password answered under the port-less
/// `portless` name, which is how a password saved before keys carried the port is
/// found (`smb_server_address::get_keychain_password`).
pub fn note_found_under_portless(server: &str, portless: &str) {
    let key = crate::network::server_identity::credential_key(server);
    FOUND_UNDER_PORTLESS
        .write_ignore_poison()
        .insert(key, portless.to_string());
}

/// Forgets every password stored for ONE SMB server: the server-level entry and
/// each share's under every name in `servers` (spelled `host:port` off 445, the
/// way `credential_key` reads them), plus a port-less entry a lookup found its
/// password under this session. Each goes from the in-memory cache too. Answers
/// how many entries were there.
///
/// An entry that isn't there is not a failure; a store that refuses a delete is.
pub fn forget_server_credentials(servers: &[String], shares: &[String]) -> Result<usize, KeychainError> {
    let mut names: Vec<String> = servers.to_vec();
    {
        let found = FOUND_UNDER_PORTLESS.read_ignore_poison();
        let recorded = servers.iter().filter_map(|server| {
            found
                .get(&crate::network::server_identity::credential_key(server))
                .cloned()
        });
        names.extend(recorded.collect::<Vec<_>>());
    }
    let mut accounts: Vec<String> = Vec::new();
    for name in &names {
        for share in std::iter::once(None).chain(shares.iter().map(|share| Some(share.as_str()))) {
            let legacy = legacy_account_name(name, share);
            for account in std::iter::once(make_account_name(name, share)).chain(legacy) {
                if !accounts.contains(&account) {
                    accounts.push(account);
                }
            }
        }
    }
    let mut gone = 0;
    for account in &accounts {
        CREDENTIAL_CACHE.write_ignore_poison().remove(account);
        match crate::secrets::store().delete(account) {
            Ok(()) => gone += 1,
            Err(SecretStoreError::NotFound(_)) => {}
            Err(e) => return Err(e.into()),
        }
    }
    {
        let mut found = FOUND_UNDER_PORTLESS.write_ignore_poison();
        for server in servers {
            found.remove(&crate::network::server_identity::credential_key(server));
        }
    }
    debug!(
        "Forgot {gone} stored SMB credential entries across {} names",
        names.len()
    );
    Ok(gone)
}

/// Whether a server-level password was already read this session (a listing or a
/// mount found it), from the in-memory cache ONLY.
///
/// ❗ Never touches the secret store: each Keychain access can raise a system
/// prompt, so a question nothing needed answered (should the share list offer
/// "Forget saved password"?) must not cost one. Unread means `false`.
pub fn has_cached_credentials(server: &str) -> bool {
    let account = make_account_name(server, None);
    CREDENTIAL_CACHE.read_ignore_poison().contains_key(&account)
}

/// The server-level credentials this session already read or saved for `server`, from
/// the in-memory cache ONLY. ❗ Never touches the secret store, so a background
/// listing can sign in as the server's account without raising a prompt.
pub fn cached_credentials(server: &str) -> Option<SmbCredentials> {
    let account = make_account_name(server, None);
    CREDENTIAL_CACHE.read_ignore_poison().get(&account).cloned()
}

/// Checks if credentials exist without retrieving them.
pub fn has_credentials(server: &str, share: Option<&str>) -> bool {
    get_credentials(server, share).is_ok()
}

/// Whether a sign-in finds a password under any of `servers`: a share's own entry
/// (each of `shares`), else the server's, the order the backend reads them in. The
/// read-side twin of [`forget_server_credentials`], over the same names.
///
/// ❗ Both count (#530): a check of the server's alone showed Remember off on a
/// share that signs in with its own.
pub fn has_server_credentials(servers: &[String], shares: &[String]) -> bool {
    servers
        .iter()
        .any(|server| shares.iter().any(|share| has_credentials(server, Some(share))) || has_credentials(server, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ❗ Answers from what a listing or mount already read, and NEVER touches the
    /// store: opening a share list must not cost a Keychain access (each can raise a
    /// system prompt) just to decide whether to show "Forget saved password".
    #[test]
    fn a_cache_only_check_sees_what_was_read_and_nothing_else() {
        let server = "cache-only-test-host:11482";
        assert!(
            !has_cached_credentials(server),
            "nothing read yet, and the store is never asked"
        );
        CREDENTIAL_CACHE.write().expect("cache").insert(
            make_account_name(server, None),
            SmbCredentials {
                username: "testuser".to_string(),
                password: "x".to_string(),
            },
        );
        assert!(has_cached_credentials(server));
        assert!(
            has_cached_credentials("CACHE-ONLY-TEST-HOST:11482"),
            "keyed like every read"
        );
    }

    /// ❗ **Forgetting a server's password takes every entry it owns, and no other
    /// server's.** Its server-level and share-level entries go, and so does a
    /// port-less entry a lookup this session found ITS password under. A port-less
    /// entry nobody found for it belongs to the server on 445 and stays.
    #[test]
    fn forgetting_a_servers_password_takes_its_entries_and_no_other_servers() {
        let _secrets = crate::test_support::isolate_secrets();
        let server = "forget-test.local:11482";
        save_credentials(server, None, "test_user_1", "a").expect("saved");
        save_credentials(server, Some("photos"), "test_user_1", "b").expect("saved");
        save_credentials("forget-test.local", None, "test_user_5", "c").expect("saved");
        save_credentials("forget-legacy.local", None, "cy", "d").expect("saved");
        note_found_under_portless("forget-legacy.local:11483", "forget-legacy.local");

        let gone = forget_server_credentials(&[server.to_string()], &["photos".to_string()]).expect("forgot");
        assert_eq!(gone, 2);
        assert!(matches!(get_credentials(server, None), Err(KeychainError::NotFound(_))));
        assert!(matches!(
            get_credentials(server, Some("photos")),
            Err(KeychainError::NotFound(_))
        ));
        assert!(!has_cached_credentials(server), "the cache lets go too");
        assert_eq!(
            get_credentials("forget-test.local", None).expect("445's own").username,
            "test_user_5",
            "the server on 445 keeps its password"
        );

        let gone = forget_server_credentials(&["forget-legacy.local:11483".to_string()], &[]).expect("forgot");
        assert_eq!(gone, 1, "the port-less entry it was found under");
        assert!(matches!(
            get_credentials("forget-legacy.local", None),
            Err(KeychainError::NotFound(_))
        ));
    }

    /// A restic repo's password is filed under the service `restic://`
    /// (`cmdr-restic`'s `KEYCHAIN_SERVICE`; the two must match), scoped
    /// `<volume id>:<repo path>`. ❗ No server name folds onto that service, so an
    /// SMB server called `restic` keeps its own entry beside a repo's, and
    /// forgetting that server's passwords never takes a backup's.
    #[test]
    fn a_restic_repo_entry_never_shares_a_key_with_an_smb_server() {
        let _secrets = crate::test_support::isolate_secrets();
        let repo_scope = "smb-testshare:/Volumes/testshare/backups/test-restic";
        save_credentials("restic://", Some(repo_scope), repo_scope, "repo-password").expect("saved");
        save_credentials("restic", None, "test_user_1", "smb-password").expect("saved");
        save_credentials("restic", Some(repo_scope), "test_user_1", "share-password").expect("saved");

        assert_ne!(
            make_account_name("restic://", Some(repo_scope)),
            make_account_name("restic", Some(repo_scope))
        );
        forget_server_credentials(&["restic".to_string()], &[repo_scope.to_string()]).expect("forgot");

        assert_eq!(
            get_credentials("restic://", Some(repo_scope))
                .expect("the repo's password survives")
                .password,
            "repo-password"
        );
    }

    #[test]
    fn test_make_account_name_server_only() {
        let account = make_account_name("TEST_SERVER", None);
        assert_eq!(account, "smb://test_server");
    }

    #[test]
    fn test_make_account_name_with_share() {
        let account = make_account_name("TEST_SERVER", Some("Documents"));
        assert_eq!(account, "smb://test_server/Documents");
    }

    /// A share-level password saved under the composed spelling has to be found by
    /// the upgrade path looking it up with the decomposed one `statfs` hands out,
    /// or the share silently falls back to guest. Same reason the server half is
    /// collapsed to a stable identity. Reported as ERR-ABXW4.
    #[test]
    fn make_account_name_folds_share_normalization() {
        let composed = make_account_name("testnas", Some("R\u{e9}gi NAS"));
        let decomposed = make_account_name("testnas", Some("Re\u{301}gi NAS"));
        assert_eq!(composed, decomposed);
    }

    #[test]
    fn test_make_account_name_case_insensitive_server() {
        let account1 = make_account_name("TEST_SERVER", Some("Share"));
        let account2 = make_account_name("test_server", Some("Share"));
        assert_eq!(account1, account2);
    }

    /// All name forms of one server must produce the same account, so a password saved
    /// under the mDNS instance name is found when the upgrade path looks it up by the
    /// `statfs` service name or the resolved hostname.
    #[test]
    fn test_make_account_name_collapses_server_name_forms() {
        let saved = make_account_name("Testnas", None);
        assert_eq!(saved, "smb://testnas");
        assert_eq!(make_account_name("Testnas.local", None), saved);
        assert_eq!(make_account_name("Testnas._smb._tcp.local", None), saved);
    }

    #[test]
    fn test_parse_password_entry() {
        let entry = make_password_entry("test_user_3", "secret123");
        let creds = parse_password_entry(&entry).unwrap();
        assert_eq!(creds.username, "test_user_3");
        assert_eq!(creds.password, "secret123");
    }

    #[test]
    fn test_parse_password_entry_with_special_chars() {
        let entry = make_password_entry("user@domain.com", "p@ss:w0rd!");
        let creds = parse_password_entry(&entry).unwrap();
        assert_eq!(creds.username, "user@domain.com");
        assert_eq!(creds.password, "p@ss:w0rd!");
    }

    #[test]
    fn test_parse_password_entry_with_null_in_password() {
        // Password containing null byte should work (only first null is separator)
        let entry = b"user\0pass\0word".to_vec();
        let creds = parse_password_entry(&entry).unwrap();
        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "pass\0word");
    }

    #[test]
    fn test_parse_password_entry_invalid() {
        let invalid = b"no-separator-here".to_vec();
        assert!(parse_password_entry(&invalid).is_none());
    }
}
