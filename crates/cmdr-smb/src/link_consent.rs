//! Which other servers a person let SMB links sign in to.
//!
//! A DFS link's target comes from the share's server, and following it signs in
//! there with the share's credentials, so a malicious or compromised server could
//! name a machine of its own and capture that sign-in. smb2 asks before it dials
//! ([`smb2::DfsTargetPolicy`]), and every session Cmdr opens answers from this set
//! ([`dfs_target_policy`]): default deny, keyed by the (share's server, target
//! host) pair.
//!
//! This holds the answers in memory; the app's `network::smb_link_consents`
//! persists them and is the only thing that adds one, after a person said yes.

use std::collections::HashSet;
use std::sync::{Arc, LazyLock, RwLock};

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use smb2::{DfsSignIn, DfsTargetPolicy};

/// The longest host name worth showing to a person: DNS's own limit.
const MAX_HOST_LEN: usize = 253;

static ALLOWED: LazyLock<RwLock<HashSet<DfsSignIn>>> = LazyLock::new(RwLock::default);

/// The policy every smb2 session of this crate carries. ❌ Never `FollowAll`, and
/// a `ClientConfig` without one refuses every different host.
pub(crate) fn dfs_target_policy() -> DfsTargetPolicy {
    DfsTargetPolicy::Ask(Arc::new(|sign_in| {
        is_presentable_host(&sign_in.target) && ALLOWED.read_ignore_poison().contains(sign_in)
    }))
}

/// The set the app fills: the one thing outside this crate may do about consent.
pub struct LinkConsents;

impl LinkConsents {
    /// Remembers that links on `server` may sign in to `target`. Answers `false`
    /// for a name no person could have read ([`is_presentable_host`]).
    pub fn allow(server: &str, target: &str) -> bool {
        if !is_presentable_host(target) {
            return false;
        }
        ALLOWED.write_ignore_poison().insert(sign_in(server, target));
        true
    }

    /// Takes the permission back. A link stops on its next call: smb2 asks again
    /// each time. Answers whether anything was there.
    pub fn forget(server: &str, target: &str) -> bool {
        ALLOWED.write_ignore_poison().remove(&sign_in(server, target))
    }

    /// Replaces the whole set, for the app's load at startup.
    pub fn replace_all(pairs: impl IntoIterator<Item = (String, String)>) {
        *ALLOWED.write_ignore_poison() = pairs
            .into_iter()
            .filter(|(_, target)| is_presentable_host(target))
            .map(|(server, target)| DfsSignIn { server, target })
            .collect();
    }
}

fn sign_in(server: &str, target: &str) -> DfsSignIn {
    DfsSignIn {
        server: server.to_string(),
        target: target.to_string(),
    }
}

/// Whether a person could read this host name and know what they're allowing.
///
/// A referral names its target in whatever characters the server likes, so one
/// could carry a bidi override or a line break to make the prompt read as another
/// host. ❌ Such a target is refused outright, with no prompt
/// (`mapping::map_smb_error`).
pub(crate) fn is_presentable_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= MAX_HOST_LEN
        && !host
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || is_invisible_format(c))
}

/// Unicode's format characters that reorder or hide text: bidi controls, zero-width
/// characters, and the byte order mark.
fn is_invisible_format(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asks(server: &str, target: &str) -> bool {
        match dfs_target_policy() {
            DfsTargetPolicy::Ask(ask) => ask(&sign_in(server, target)),
            other => panic!("expected Ask, got {other:?}"),
        }
    }

    #[test]
    fn no_consent_means_no() {
        assert!(!asks("deny.server.test", "deny.target.test"));
    }

    #[test]
    fn consent_is_for_the_pair_and_can_be_forgotten() {
        assert!(LinkConsents::allow("pair.server.test", "pair.target.test"));
        assert!(asks("pair.server.test", "pair.target.test"));
        assert!(!asks("other.server.test", "pair.target.test"), "another share's server");
        assert!(!asks("pair.server.test", "other.target.test"), "another target");

        assert!(LinkConsents::forget("pair.server.test", "pair.target.test"));
        assert!(!asks("pair.server.test", "pair.target.test"));
        assert!(!LinkConsents::forget("pair.server.test", "pair.target.test"));
    }

    #[test]
    fn a_name_a_person_couldnt_read_is_never_allowed() {
        for host in ["", "nas\u{202E}lave", "nas\nevil", "nas evil", "nas\u{200B}", "a\u{0}b"] {
            assert!(!is_presentable_host(host), "{host:?}");
            assert!(!LinkConsents::allow("sneaky.server.test", host), "{host:?}");
        }
        assert!(!is_presentable_host(&"a".repeat(MAX_HOST_LEN + 1)));
        assert!(is_presentable_host("fs01.example.com"));
        assert!(is_presentable_host("192.168.1.5"));
        assert!(is_presentable_host("szerver-ű"));
    }
}
