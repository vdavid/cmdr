//! The top-level conflict question, answered from ONE destination listing.
//!
//! The concurrent copy driver used to ask `dest_volume.get_metadata` once per
//! top-level source, serialized: 74% of a 500-file NAS copy
//! (`docs/notes/transfer-concurrency-window-bench-2026-08-02.md`). Phase 0.6's
//! stale-temp reap already lists that same directory, so the round trip is
//! spent either way; this turns its result into the answer.
//!
//! **A name lookup in a listing is not the same question as `get_metadata`**,
//! and every gap between them is a conflict that silently becomes an overwrite.
//! So this index answers [`DestLookup::Absent`] only for a name no backend could
//! resolve onto an entry it holds; anything it can't settle comes back
//! [`DestLookup::Unknown`] and the caller pays the probe it would have paid
//! anyway. Wrong-way-round is cheap (one round trip), wrong-way-forward is the
//! user's file. Rationale and the residual list: `DETAILS.md` § "Answering the
//! pre-check from one listing".

use std::collections::HashMap;
use std::ffi::OsStr;

use unicode_normalization::UnicodeNormalization;

use crate::file_system::listing::FileEntry;

/// What one destination listing can say about a name a copy is about to write.
pub(super) enum DestLookup {
    /// No entry in the listing can resolve to this name on any backend. The
    /// caller may treat the name as free without asking.
    Absent,
    /// The listing holds this exact name, carrying the same `size` /
    /// `is_directory` a `get_metadata` probe would have returned.
    Present(Box<FileEntry>),
    /// The listing can't settle this name. Ask the backend.
    Unknown,
}

/// A name index over ONE destination listing, matched the way real backends
/// resolve names rather than byte-for-byte.
///
/// Entries are bucketed under a folded key (NFC + lowercase), so a name that
/// collides with a stored one under EITHER case-insensitivity (SMB shares,
/// macOS volumes) or Unicode normalization (macOS and SMB move paths between
/// NFC and NFD) lands in the same bucket. Within a bucket, only a byte-exact
/// name is answered from memory; a fold-only match is [`DestLookup::Unknown`],
/// because whether the two names are the same file is the destination
/// filesystem's call, not ours.
pub(super) struct DestNameIndex {
    by_folded: HashMap<String, Vec<FileEntry>>,
}

/// The key two names share when a case- or normalization-insensitive backend
/// would treat them as one.
///
/// NFC to match what `SmbVolume::to_smb_path` already sends on the wire, then
/// lowercase. The ASCII fast path is the same answer for ASCII input (NFC is
/// identity there, and `char::to_lowercase` is ASCII lowercase) without
/// allocating through the normalizer for the overwhelmingly common case.
///
/// `pub(super)` because the volume conflict resolver asks it about the final
/// component of two paths, to tell a duplicate from a clash
/// (`volume/conflict.rs::is_the_same_volume_path`). ❌ It is never asked about a
/// whole path: this rule speaks for names inside ONE listing, and no listing
/// says whether two differently-cased parent directories are one directory.
/// One folding rule for the whole transfer layer, one question it answers.
pub(super) fn fold(name: &str) -> String {
    if name.is_ascii() {
        return name.to_ascii_lowercase();
    }
    name.nfc().flat_map(char::to_lowercase).collect()
}

impl DestNameIndex {
    pub(super) fn build(entries: Vec<FileEntry>) -> Self {
        let mut by_folded: HashMap<String, Vec<FileEntry>> = HashMap::with_capacity(entries.len());
        for entry in entries {
            by_folded.entry(fold(&entry.name)).or_default().push(entry);
        }
        Self { by_folded }
    }

    /// Answers whether `name` is already taken at the destination.
    ///
    /// `None` (a source path with no final component) is [`DestLookup::Unknown`]:
    /// the destination that copy targets is the directory itself, which this
    /// index doesn't describe.
    pub(super) fn lookup(&self, name: Option<&OsStr>) -> DestLookup {
        // A non-UTF-8 name can't be folded the way the backend would, so it
        // isn't ours to answer.
        let Some(name) = name.and_then(OsStr::to_str) else {
            return DestLookup::Unknown;
        };
        if let Some(bucket) = self.by_folded.get(&fold(name)) {
            return match bucket.iter().find(|entry| entry.name == name) {
                Some(entry) => DestLookup::Present(Box::new(entry.clone())),
                None => DestLookup::Unknown,
            };
        }
        if self.may_resolve_to_a_stored_name(name) {
            return DestLookup::Unknown;
        }
        DestLookup::Absent
    }

    /// Whether a backend could still route `name` onto an entry we hold, by a
    /// rule the fold above doesn't capture. Each of these costs one probe when
    /// it fires and nothing when it doesn't, so they lean generous.
    fn may_resolve_to_a_stored_name(&self, name: &str) -> bool {
        // Win32 path canonicalization drops trailing dots and spaces from the
        // REQUEST, so a Windows-hosted share resolves `report.` to `report`.
        let trimmed = name.trim_end_matches(['.', ' ']);
        if trimmed != name && !trimmed.is_empty() && self.by_folded.contains_key(&fold(trimmed)) {
            return true;
        }
        // 8.3 short names (`PROGRA~1`) are a second, generated name for an entry
        // whose real name a listing reports — an alias namespace we can't
        // enumerate, so we can't prove a miss. Names carrying a `~` are rare
        // enough that conceding the probe costs nothing measurable.
        name.contains('~')
    }
}

#[cfg(test)]
#[path = "dest_name_index_tests.rs"]
mod tests;
