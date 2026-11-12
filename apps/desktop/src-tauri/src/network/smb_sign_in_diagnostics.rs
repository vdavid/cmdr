//! What a refused SMB sign-in logs: WARN, with the credential's SHAPE and where it came from,
//! never a value or a length. "Wrong password" versus "the account needs its domain" versus
//! "a stale saved entry" is most of an SMB sign-in triage, and each has a shape.

use super::smb_client::{ShareListError, ShareListResult};
use serde::Deserialize;

/// Where the credentials a listing signs in with came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CredentialSource {
    /// Typed into the sign-in sheet for this attempt.
    Typed,
    /// A saved entry: the Keychain, or this session's copy of one.
    Saved,
}

/// The privacy-safe shape of a username and password: booleans and a domain form only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CredentialShape {
    user_empty: bool,
    /// `DOMAIN\user` (a backslash) or `user@domain` (an `@`), the two forms a NAS may insist on.
    user_domain: UserDomain,
    password_empty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UserDomain {
    None,
    Backslash,
    At,
}

impl CredentialShape {
    pub fn of(user: &str, password: &str) -> Self {
        let user_domain = if user.contains('\\') {
            UserDomain::Backslash
        } else if user.contains('@') {
            UserDomain::At
        } else {
            UserDomain::None
        };
        Self {
            user_empty: user.trim().is_empty(),
            user_domain,
            password_empty: password.is_empty(),
        }
    }
}

impl std::fmt::Display for CredentialShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let domain = match self.user_domain {
            UserDomain::None => "none",
            UserDomain::Backslash => "backslash",
            UserDomain::At => "at",
        };
        write!(
            f,
            "user_empty={}, user_domain={domain}, password_empty={}",
            self.user_empty, self.password_empty
        )
    }
}

/// Log a listing that signed in with credentials and was refused, at WARN. Anything else (a
/// success, a timeout, a guest listing) logs nothing here.
pub fn log_sign_in_refusal(
    host: &str,
    port: u16,
    credentials: Option<(&str, &str)>,
    source: CredentialSource,
    result: &Result<ShareListResult, ShareListError>,
) {
    let Some((user, password)) = credentials else { return };
    let refusal = match result {
        Err(ShareListError::AuthFailed { .. }) => "auth_failed",
        Err(ShareListError::AuthRequired { .. }) => "auth_required",
        _ => return,
    };
    log::warn!(
        target: "smb_sign_in",
        // allowed-log-private: `source` is the `CredentialSource` variant, not a path
        "SMB sign-in refused: host={host:?}, port={port}, refusal={refusal}, source={source:?}, {}",
        // allowed-log-private: the credential's shape (empty, domain form), never the user name itself
        CredentialShape::of(user, password),
        host = cmdr_fs::log_private::LogPrivate::host(host),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shape_says_empty_and_domain_form_and_nothing_else() {
        assert_eq!(
            CredentialShape::of("test_user_1", "secret").to_string(),
            "user_empty=false, user_domain=none, password_empty=false"
        );
        assert_eq!(
            CredentialShape::of(r"WORKGROUP\test_user_1", "").to_string(),
            "user_empty=false, user_domain=backslash, password_empty=true"
        );
        assert_eq!(
            CredentialShape::of("test_user_1@example.test", "x").to_string(),
            "user_empty=false, user_domain=at, password_empty=false"
        );
        assert_eq!(
            CredentialShape::of("  ", "x").to_string(),
            "user_empty=true, user_domain=none, password_empty=false"
        );
        let shape = CredentialShape::of("test_user_1@example.test", "hunter2").to_string();
        for private in ["test_user_1", "example", "hunter2", "7"] {
            assert!(!shape.contains(private), "{private:?} in {shape}");
        }
    }
}
