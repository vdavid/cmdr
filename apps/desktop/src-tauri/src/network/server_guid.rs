//! Which SMB server a share lives on: the GUID the server answers NEGOTIATE with.
//!
//! One NAS reached by two addresses (a LAN IP and a Tailscale one) mounts as two
//! volumes with two ids, and only the server's own GUID says they're one box. The
//! share name pairs with it to prove two saved shares are one share
//! (`location::same_share`); the GUID alone names the BOX, never the share.
//!
//! ❗ An all-zero GUID is no GUID: smb2 fills `Guid::ZERO` before a negotiate
//! lands, and a server may answer it. [`ServerGuid`] can't hold one, so two rows
//! that never learned their server can't match on it.

use serde::{Deserialize, Serialize};
use smb2::pack::Guid;

/// A server's non-zero GUID, in its canonical lowercase text form
/// (`8-4-4-4-12` hex digits, no braces): what the saved-share store keeps.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServerGuid(String);

impl ServerGuid {
    /// The GUID a NEGOTIATE answered with, or `None` for the all-zero one.
    pub fn from_negotiated(guid: Guid) -> Option<Self> {
        if guid == Guid::ZERO {
            return None;
        }
        let tail: String = guid.data4.iter().map(|b| format!("{b:02x}")).collect();
        Some(Self(format!(
            "{:08x}-{:04x}-{:04x}-{}-{}",
            guid.data1,
            guid.data2,
            guid.data3,
            &tail[..4],
            &tail[4..]
        )))
    }

    /// A stored GUID read back, or `None` when it isn't one (malformed, or all
    /// zeros: a hand-edited store must not make two shares match).
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().to_ascii_lowercase();
        let groups: Vec<&str> = text.split('-').collect();
        let shape_ok = groups.iter().map(|g| g.len()).eq([8, 4, 4, 4, 12])
            && groups.iter().all(|g| g.bytes().all(|b| b.is_ascii_hexdigit()));
        let all_zero = text.bytes().all(|b| b == b'0' || b == b'-');
        (shape_ok && !all_zero).then_some(Self(text))
    }

    /// The canonical text form.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How a server GUID was learned. It decides whether the GUID may replace one a
/// row already holds, and whether it proves enough to let one member of a share
/// answer for another (`location::same_share`). Stored beside the GUID as
/// `serverGuidProof`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum GuidProof {
    /// From a session that signed in as the share's account: the latest truth
    /// about the box, so it overwrites (a NAS restored or replaced mints a new GUID).
    Authenticated,
    /// From a credential-free NEGOTIATE probe of a saved address, or a guest
    /// session: whatever answers there may not be the share's server (a hotel
    /// network can hand out the saved LAN IP), so it only FILLS a missing GUID,
    /// and only ever makes a `sameShareAs` hint. The default: a row saved before
    /// Cmdr kept the proof can't show a sign-in recorded its GUID.
    #[default]
    Unauthenticated,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Guid {
        Guid {
            data1: 0x6ba7_b810,
            data2: 0x9dad,
            data3: 0x11d1,
            data4: [0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8],
        }
    }

    #[test]
    fn an_all_zero_guid_is_no_guid() {
        assert_eq!(ServerGuid::from_negotiated(Guid::ZERO), None);
        assert_eq!(ServerGuid::parse("00000000-0000-0000-0000-000000000000"), None);
    }

    #[test]
    fn a_negotiated_guid_reads_back_as_itself() {
        let guid = ServerGuid::from_negotiated(sample()).expect("non-zero");
        assert_eq!(guid.as_str(), "6ba7b810-9dad-11d1-80b4-00c04fd430c8");
        assert_eq!(ServerGuid::parse(guid.as_str()), Some(guid.clone()));
        assert_eq!(
            ServerGuid::parse("6BA7B810-9DAD-11D1-80B4-00C04FD430C8"),
            Some(guid),
            "case doesn't make a second server"
        );
    }

    #[test]
    fn a_malformed_stored_guid_is_no_guid() {
        for text in [
            "",
            "testnas",
            "{6ba7b810-9dad-11d1-80b4-00c04fd430c8}",
            "6ba7b810-9dad-11d1-80b4",
        ] {
            assert_eq!(ServerGuid::parse(text), None, "{text:?}");
        }
    }
}
