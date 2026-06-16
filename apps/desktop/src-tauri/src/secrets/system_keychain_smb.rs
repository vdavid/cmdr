//! Reading SMB passwords that **another app** (Finder / macOS) saved in the login keychain.
//!
//! When you connect to an SMB share in Finder with "Remember this password in my
//! keychain", macOS stores an *internet-password* item (`kSecClassInternetPassword`,
//! protocol `smb`) keyed by the server name Finder used — typically the Bonjour service
//! name (`Naspolya._smb._tcp.local`) — with the username in the item. Cmdr's own secret
//! store ([`super::keychain_macos`]) holds *generic* passwords under our own service name
//! and can't see these. This module reads Finder's item so we can connect a share the
//! user already authenticated, without making them retype the password.
//!
//! ## Consent
//!
//! Reading the *attributes* of another app's keychain item (does it exist? what account?)
//! does NOT prompt. Reading the *secret data* (the password) triggers macOS's consent
//! dialog ("Cmdr wants to use confidential information stored in … in your keychain") —
//! drawn by SecurityAgent, whose text we can't customize. "Always Allow" adds Cmdr to the
//! item's ACL so it's silent thereafter. So [`account_for_any`] probes silently (drives
//! the "offer the button?" decision) and [`read_password`] does the consent-gated read
//! only when the user explicitly asks for it. **Never call `read_password` automatically
//! at startup** — that would pop a system dialog per share before the user has context
//! (the FDA-popup-storm lesson).
//!
//! ## Identity
//!
//! The same NAS is keyed by whatever name the saving app used, which is rarely the form
//! Cmdr mounts by (we prefer the IP). [`server_query_candidates`] builds the ordered set
//! of names to try (mount server, resolved hostname, discovery aliases); the first item
//! with a *real* account wins ([`is_real_account`] skips macOS's "No user account" guest
//! sentinel). macOS-only.

use core_foundation::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::boolean::kCFBooleanTrue;
use core_foundation::data::{CFData, CFDataRef};
use core_foundation::dictionary::{
    CFDictionaryCreateMutable, CFDictionaryGetValue, CFDictionaryRef, CFDictionarySetValue, CFMutableDictionaryRef,
    kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks,
};
use core_foundation::string::{CFString, CFStringRef};
use security_framework_sys::item::{
    kSecAttrAccount, kSecAttrServer, kSecClass, kSecClassInternetPassword, kSecMatchLimit, kSecMatchLimitAll,
    kSecReturnAttributes, kSecReturnData,
};
use security_framework_sys::keychain_item::SecItemCopyMatching;
use std::ffi::c_void;
use std::ptr;

/// Credentials read from another app's keychain item.
pub struct SmbSavedCredentials {
    pub username: String,
    pub password: String,
}

/// Ordered, de-duplicated set of server strings to query the keychain with. The saving
/// app (Finder) keys the item by the name *it* used — usually the mDNS service name — so
/// we try the mount server, the resolved hostname, and any discovery aliases in turn.
pub fn server_query_candidates(mount_server: &str, hostname: Option<&str>, aliases: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        let s = s.trim();
        if !s.is_empty() && !out.iter().any(|x| x == s) {
            out.push(s.to_string());
        }
    };
    push(mount_server);
    if let Some(h) = hostname {
        push(h);
    }
    for a in aliases {
        push(a);
    }
    out
}

/// `"No user account"` is macOS's sentinel for a guest / account-less internet-password
/// item — not a usable login. Empty accounts are likewise unusable.
pub fn is_real_account(account: &str) -> bool {
    let a = account.trim();
    !a.is_empty() && a != "No user account"
}

/// Returns `(server, account)` for the first candidate with a real account, **without**
/// triggering the consent dialog (attributes only). Drives the "offer the saved-password
/// button?" decision.
pub fn account_for_any(candidates: &[String]) -> Option<(String, String)> {
    candidates
        .iter()
        .find_map(|server| account_for_server(server).map(|account| (server.clone(), account)))
}

/// Reads the password for the first candidate that has a real account. The data read
/// triggers the macOS consent dialog. Returns `None` if nothing is stored or the user
/// denies access.
pub fn read_password(candidates: &[String]) -> Option<SmbSavedCredentials> {
    for server in candidates {
        if let Some(account) = account_for_server(server)
            && let Some(password) = password_for(server, &account)
        {
            return Some(SmbSavedCredentials {
                username: account,
                password,
            });
        }
    }
    None
}

/// Builds a mutable query dict for an SMB internet-password on `server`. Caller adds the
/// return-mode + limit keys and owns releasing the returned dict.
fn base_query(server: &str) -> CFMutableDictionaryRef {
    // SAFETY: `CFDictionaryCreateMutable` with the null allocator and kCFType key/value callbacks
    // returns an owning (+1) dictionary whose callbacks retain every key/value on `SetValue`. The
    // `kSec*` constants are static CFType refs, and `server_cf` is a live CFString held across its
    // `SetValue` call, so the dictionary's retain keeps the server value alive after it drops.
    // Ownership of the +1 dictionary transfers to the caller, who must `CFRelease` it.
    unsafe {
        let dict = CFDictionaryCreateMutable(
            ptr::null(),
            0,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        CFDictionarySetValue(
            dict,
            kSecClass as *const c_void,
            kSecClassInternetPassword as *const c_void,
        );
        let server_cf = CFString::new(server);
        CFDictionarySetValue(
            dict,
            kSecAttrServer as *const c_void,
            server_cf.as_concrete_TypeRef() as *const c_void,
        );
        dict
    }
}

/// Attribute-only lookup of the account for an SMB item on `server`. No consent.
fn account_for_server(server: &str) -> Option<String> {
    // SAFETY: `base_query` returns a +1 dictionary; we add return-mode/limit keys (static CFType
    // refs) and pass it to `SecItemCopyMatching`, then `CFRelease` the query dict on every exit
    // path. `result` is a valid out-param; we deref it only when `status == 0 && !result.is_null()`.
    // The CFArray and per-item CFDictionary/CFString are borrowed under the Get rule (no extra
    // retain, so no extra release), and the +1 `result` array is released once via `CFRelease`.
    unsafe {
        let dict = base_query(server);
        CFDictionarySetValue(
            dict,
            kSecReturnAttributes as *const c_void,
            kCFBooleanTrue as *const c_void,
        );
        CFDictionarySetValue(
            dict,
            kSecMatchLimit as *const c_void,
            kSecMatchLimitAll as *const c_void,
        );

        let mut result: CFTypeRef = ptr::null();
        let status = SecItemCopyMatching(dict as CFDictionaryRef, &mut result);
        CFRelease(dict as CFTypeRef);

        if status != 0 || result.is_null() {
            return None;
        }

        let arr = result as CFArrayRef;
        let count = CFArrayGetCount(arr);
        let mut found = None;
        for i in 0..count {
            let item = CFArrayGetValueAtIndex(arr, i) as CFDictionaryRef;
            let acct_ref = CFDictionaryGetValue(item, kSecAttrAccount as *const c_void) as CFStringRef;
            if !acct_ref.is_null() {
                let account = CFString::wrap_under_get_rule(acct_ref).to_string();
                if is_real_account(&account) {
                    found = Some(account);
                    break;
                }
            }
        }
        CFRelease(result);
        found
    }
}

/// Consent-gated data read of the password for a specific `server` + `account`.
fn password_for(server: &str, account: &str) -> Option<String> {
    // SAFETY: `base_query` returns a +1 dictionary; `account_cf` is a live CFString held across its
    // `SetValue`, and the query dict is `CFRelease`d on every exit path. `result` is a valid
    // out-param, deref'd only when `status == 0 && !result.is_null()`. `SecItemCopyMatching` returns
    // the CFData under the Create rule, so `wrap_under_create_rule` takes that single ownership and
    // releases it once on drop.
    unsafe {
        let dict = base_query(server);
        let account_cf = CFString::new(account);
        CFDictionarySetValue(
            dict,
            kSecAttrAccount as *const c_void,
            account_cf.as_concrete_TypeRef() as *const c_void,
        );
        CFDictionarySetValue(dict, kSecReturnData as *const c_void, kCFBooleanTrue as *const c_void);
        // No `kSecMatchLimit`: the SecItem default is a single match, which is what we
        // want here (one server+account item). (`kSecMatchLimitOne` isn't exported.)

        let mut result: CFTypeRef = ptr::null();
        let status = SecItemCopyMatching(dict as CFDictionaryRef, &mut result);
        CFRelease(dict as CFTypeRef);

        if status != 0 || result.is_null() {
            return None;
        }

        // `SecItemCopyMatching` returns under the create rule; `wrap_under_create_rule`
        // takes ownership so the CFData is released when it drops.
        let data = CFData::wrap_under_create_rule(result as CFDataRef);
        String::from_utf8(data.bytes().to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_are_ordered_and_deduped() {
        let aliases = vec!["Naspolya._smb._tcp.local".to_string(), "192.168.1.111".to_string()];
        let got = server_query_candidates("192.168.1.111", Some("naspolya.local"), &aliases);
        assert_eq!(got, vec!["192.168.1.111", "naspolya.local", "Naspolya._smb._tcp.local"]);
    }

    #[test]
    fn candidates_skip_blanks_and_none_hostname() {
        let got = server_query_candidates("naspolya", None, &[String::new(), "  ".to_string()]);
        assert_eq!(got, vec!["naspolya"]);
    }

    #[test]
    fn real_account_rejects_the_guest_sentinel() {
        assert!(is_real_account("david"));
        assert!(!is_real_account("No user account"));
        assert!(!is_real_account(""));
        assert!(!is_real_account("   "));
    }
}
