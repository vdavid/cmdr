//! One share reached by two addresses is ONE share: which volumes are proven the
//! same, and which of them may answer a read addressed to another.
//!
//! A NAS mounted over its LAN IP and over Tailscale is two volumes with two ids,
//! two indexes, and two importance stores. Two rules, both proof-based
//! (`tools/DETAILS.md` § The location contract):
//!
//! - **Same share** (`sameShareAs`): both record the same server GUID (non-zero:
//!   `ServerGuid` can't hold the all-zero one) AND the same share name, ignoring
//!   case. The GUID names the BOX, so it never groups alone. ❌ No fuzzy "possibly
//!   the same": an unproven pair isn't grouped.
//! - **Redirectable**: the same share, signed in as the same account, with BOTH
//!   GUIDs recorded by a session that signed in (`GuidProof::Authenticated`).
//!   Another account can see different files on the same share, and a GUID the
//!   probe or a guest session filled is only whatever answered a saved address,
//!   so either sibling stays a `sameShareAs` hint and never answers for (or takes
//!   a write meant for) the volume asked.
//!
//! Pure over [`VolumeFacts`], each fact carrying its `ShareIdentity`
//! (`share_identity.rs`); `facts.rs` reads that off the saved-share store
//! (`serverGuid`, `serverGuidProof`) and the live sessions
//! (`network::smb_server_guid`).

use cmdr_fs::volume::ConnectionState;
use cmdr_fs::volume::app_paths::rebase;
use serde::Serialize;

use super::Location;
use super::resolve::{ResolvedLocation, VolumeFact, VolumeFacts};

/// The volume a read or a row was asked about, when another member of its share
/// answered in its place: `{ "volumeId": … }` beside the answer's `location`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Via {
    pub volume_id: String,
}

/// How well a share member can answer a read, best first. A folder-independent
/// order; `list_dir` refines it per folder (`read/listing.rs`).
fn standing(volume: &VolumeFact) -> (u8, u64) {
    match (volume.live, volume.index.live, volume.index.stored) {
        (true, true, _) => (3, 0),
        (true, false, _) => (2, 0),
        (false, _, true) => (1, volume.index.stored_as_of.unwrap_or(0)),
        (false, _, false) => (0, 0),
    }
}

impl VolumeFacts {
    /// Every other volume proven to be the same share as `id`, in listing order.
    pub(crate) fn same_share_as(&self, id: &str) -> Vec<String> {
        let Some(identity) = self.get(id).and_then(|v| v.share.as_ref()) else {
            return Vec::new();
        };
        self.volumes
            .iter()
            .filter(|v| v.id != id && v.share.as_ref().is_some_and(|other| identity.same_share(other)))
            .map(|v| v.id.clone())
            .collect()
    }

    /// The members of `id`'s share that may answer a read on it, `id` first:
    /// itself, then every redirectable sibling.
    fn redirect_group(&self, id: &str) -> Vec<&VolumeFact> {
        let Some(asked) = self.get(id) else {
            return Vec::new();
        };
        let siblings = self
            .volumes
            .iter()
            .filter(|v| v.id != id && matches!((&asked.share, &v.share), (Some(a), Some(b)) if a.redirectable(b)));
        std::iter::once(asked).chain(siblings).collect()
    }

    /// `resolved` re-addressed onto every member that may answer for it, the
    /// volume asked first. Each sibling's location is the same place under its own
    /// root, with `via` naming the volume asked.
    pub(crate) fn readers(&self, resolved: &ResolvedLocation) -> Vec<ResolvedLocation> {
        let asked = &resolved.location.volume_id;
        let mut readers = vec![resolved.clone()];
        for member in self.redirect_group(asked).into_iter().skip(1) {
            let Some(path) = rebase(&resolved.location.path, &resolved.root, &member.root) else {
                continue;
            };
            readers.push(ResolvedLocation {
                location: Location {
                    volume_id: member.id.clone(),
                    path,
                },
                name: member.name.clone(),
                kind: member.kind,
                root: member.root.clone(),
                live: member.live,
                index: member.index.clone(),
                rebased_from: resolved.rebased_from.clone(),
                via: Some(asked.clone()),
            });
        }
        readers
    }

    /// The member of `id`'s share a place on it is best addressed on: live with an
    /// index, then live, then the freshest index on disk, `id` itself on a tie (or
    /// when it has no redirectable sibling). What `important_folders` re-addresses
    /// a member's rows onto.
    pub(crate) fn preferred_member(&self, id: &str) -> Option<&VolumeFact> {
        self.redirect_group(id)
            .into_iter()
            .enumerate()
            .max_by_key(|(order, member)| (standing(member), std::cmp::Reverse(*order)))
            .map(|(_, member)| member)
    }

    /// `id` and its redirectable siblings, the scope a read naming `id` spans.
    pub(crate) fn redirect_group_ids(&self, id: &str) -> Vec<String> {
        self.redirect_group(id).into_iter().map(|v| v.id.clone()).collect()
    }

    /// The member of `id`'s share a connect should act on: `id` itself when it's
    /// serving, else a redirectable sibling that is (answered as already connected,
    /// no dial), else the member connected most recently per `last_connected`
    /// (seconds since the epoch), `id` on a tie or when none says. Recency is the
    /// one signal for "the address that works from here"; nothing sniffs networks.
    /// `None` for an id the facts don't hold.
    pub(crate) fn connect_target(&self, id: &str, last_connected: impl Fn(&str) -> Option<i64>) -> Option<&VolumeFact> {
        let group = self.redirect_group(id);
        if let Some(serving) = group.iter().find(|member| serving(member)) {
            return Some(serving);
        }
        group
            .into_iter()
            .enumerate()
            .max_by_key(|(order, member)| (last_connected(&member.id), std::cmp::Reverse(*order)))
            .map(|(_, member)| member)
    }
}

/// Registered and answering: no session to dial and no reconnect to wait for.
/// A registered volume with no connection state is a local disk, always serving.
fn serving(volume: &VolumeFact) -> bool {
    volume.live && volume.connection_state.is_none_or(ConnectionState::is_live)
}

#[cfg(test)]
#[path = "same_share_tests.rs"]
mod tests;
