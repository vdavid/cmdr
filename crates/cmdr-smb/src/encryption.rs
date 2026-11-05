//! Whether every SMB session this crate opens must be encrypted.
//!
//! One process-wide switch, because the app's setting is global ("Always require
//! SMB encryption") and encryption is decided per session anyway. Every
//! `ClientConfig` this crate builds starts from [`session_config`], which reads
//! the switch at the moment the session is built: a new volume, a reconnect, a
//! scan-pool or transfer session, the watcher, a share listing, and the
//! `try_open_share` probe all follow the current value.
//!
//! ❗ A session that's already up keeps what it negotiated. Flipping the switch
//! never tears one down (that would cut a running copy); it takes effect the next
//! time a session is built, a reconnect included.
//!
//! The app's `network::smb_encryption` is the only caller of
//! [`SmbEncryption::set_required`]: it ORs the person's setting with the managed
//! policy's `RequireSMBEncryption`.

use std::sync::atomic::{AtomicBool, Ordering};

use smb2::ClientConfig;

static REQUIRED: AtomicBool = AtomicBool::new(false);

/// The switch the app sets: the one thing outside this crate may do about encryption.
pub struct SmbEncryption;

impl SmbEncryption {
    /// Requires encryption on every session built from now on, or stops requiring
    /// it. Sessions already up keep theirs (module docs).
    pub fn set_required(required: bool) {
        REQUIRED.store(required, Ordering::Release);
    }

    /// Whether sessions built now must be encrypted.
    pub fn is_required() -> bool {
        REQUIRED.load(Ordering::Acquire)
    }
}

/// What every `ClientConfig` this crate builds starts from (`..session_config()`
/// in place of `..Default::default()`): smb2's defaults plus the two things every
/// session must carry, the encryption requirement and the DFS link consent
/// policy. ❌ Never `..Default::default()` in a non-test config: that session
/// would skip the requirement, and `every_client_config_starts_from_session_config`
/// fails on one.
pub(crate) fn session_config() -> ClientConfig {
    ClientConfig {
        require_encryption: SmbEncryption::is_required(),
        dfs_target_policy: crate::link_consent::dfs_target_policy(),
        ..Default::default()
    }
}

/// Sets the switch for one test and restores it when the test ends, pass or
/// fail. Nextest runs each test in its own process, so no other test sees the
/// flip meanwhile.
#[cfg(test)]
pub(crate) struct RequiredForTest;

#[cfg(test)]
impl RequiredForTest {
    pub(crate) fn set(required: bool) -> Self {
        SmbEncryption::set_required(required);
        Self
    }
}

#[cfg(test)]
impl Drop for RequiredForTest {
    fn drop(&mut self) {
        SmbEncryption::set_required(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_config_carries_the_switch_as_it_stands_when_built() {
        let _reset = RequiredForTest::set(false);
        assert!(!session_config().require_encryption, "off by default");
        SmbEncryption::set_required(true);
        assert!(session_config().require_encryption);
        SmbEncryption::set_required(false);
        assert!(!session_config().require_encryption);
    }

    /// The regression guard for "every session Cmdr opens": a new `ClientConfig`
    /// literal that fills its rest from `Default::default()` would open a session
    /// that ignores the setting. Scans this crate's non-test sources.
    #[test]
    fn every_client_config_starts_from_session_config() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        let mut literals = 0;
        visit(&src, &mut |path, text| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if name.ends_with("_test.rs") || name == "encryption.rs" {
                return;
            }
            // Only the code above a file's `#[cfg(test)] mod tests`.
            let code = text.split("#[cfg(test)]\nmod tests").next().unwrap_or(text);
            for (start, _) in code.match_indices("ClientConfig {") {
                literals += 1;
                let rest = &code[start..];
                let end = rest.find("};").unwrap_or(rest.len());
                if !rest[..end].contains("..crate::encryption::session_config()") {
                    offenders.push(format!("{}:{start}", path.display()));
                }
            }
        });
        assert!(
            literals >= 5,
            "the scan saw too few configs ({literals}); is it reading the sources?"
        );
        assert!(
            offenders.is_empty(),
            "these `ClientConfig`s skip `..crate::encryption::session_config()`: {offenders:?}"
        );
    }

    fn visit(dir: &std::path::Path, f: &mut dyn FnMut(&std::path::Path, &str)) {
        for entry in std::fs::read_dir(dir).expect("read src dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, f);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).expect("read source");
                f(&path, &text);
            }
        }
    }
}
