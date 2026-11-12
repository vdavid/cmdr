//! Which server, share, and account an SMB volume is: the proof `same_share.rs`
//! groups volumes by. Its own module because every volume fact carries one
//! (`resolve.rs`) and the grouping reads the facts: in either of those two it
//! would weld them into a cycle.

use crate::network::server_guid::{GuidProof, ServerGuid};

/// What proves which share a volume is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShareIdentity {
    /// `None` until a session or the probe learned it; with how it was learned.
    server_guid: Option<(ServerGuid, GuidProof)>,
    /// Case- and NFC-folded, so `testshare` and `TESTSHARE` are one share.
    share_name: String,
    /// The account the share is opened as, folded; `None` for a guest.
    username: Option<String>,
}

impl ShareIdentity {
    /// An identity from the stored fields: a GUID that doesn't parse (malformed or
    /// all zeros) is no GUID, and a `Guest` sign-in is no account.
    pub(crate) fn new(server_guid: Option<(&str, GuidProof)>, share_name: &str, username: Option<&str>) -> Self {
        Self {
            server_guid: server_guid.and_then(|(guid, proof)| Some((ServerGuid::parse(guid)?, proof))),
            share_name: fold(share_name),
            username: crate::network::known_shares::share_account(username),
        }
    }

    /// Proven the same share: one server GUID, one share name. A hint whichever
    /// way each GUID was learned.
    pub(crate) fn same_share(&self, other: &Self) -> bool {
        matches!((&self.server_guid, &other.server_guid), (Some((a, _)), Some((b, _))) if a == b)
            && self.share_name == other.share_name
    }

    /// The same share, opened as the same account, with BOTH GUIDs recorded by a
    /// signed-in session: a read on one may be answered by the other.
    pub(crate) fn redirectable(&self, other: &Self) -> bool {
        self.same_share(other) && self.signed_in() && other.signed_in() && self.username == other.username
    }

    /// Whether a session that signed in as the share's account recorded the GUID.
    fn signed_in(&self) -> bool {
        matches!(self.server_guid, Some((_, GuidProof::Authenticated)))
    }
}

fn fold(name: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    name.nfc().flat_map(char::to_lowercase).collect()
}
