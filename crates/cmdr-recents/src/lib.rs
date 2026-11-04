//! The rules of a most-recently-used list: newest first, one row per distinct thing,
//! a cap from the oldest end, and ids that stay unique.
//!
//! Pure `Vec` work with no I/O, so every host keeps its list the same way: the desktop
//! app persists one per dialog (`apps/desktop/src-tauri/src/recents/`, which owns the
//! file, the locks, and the quarantine), and a browser host keeps one for the
//! session. `CLAUDE.md` has the must-knows.
#![warn(unused_crate_dependencies)]

/// One entry in a recents list: the facts the list rules read.
pub trait RecentItem {
    /// The entry's unique id, which a caller removes by.
    fn id(&self) -> &str;

    /// Replaces the id. Called only when an added entry's id is one the list already
    /// holds, so two rows can't share one.
    fn set_id(&mut self, id: String);

    /// What makes two entries "the same thing". Adding an entry drops every earlier
    /// one with an equal key, so the newest copy ends up on top.
    fn dedupe_key(&self) -> String;
}

/// Puts `entry` on top, dropping every earlier entry with the same dedupe key, then
/// trims the tail to `max_count`. A cap of zero means no list at all: it empties the
/// list and drops `entry` too.
///
/// `fresh_id` mints an id for `entry` when its own is already taken. It's the host's,
/// because minting one reads an RNG or a clock, which a browser host answers
/// differently from the desktop (`SystemTime::now` panics there).
pub fn add_to<E: RecentItem>(entries: &mut Vec<E>, mut entry: E, max_count: usize, fresh_id: impl FnOnce() -> String) {
    if max_count == 0 {
        entries.clear();
        return;
    }

    // Every earlier copy, not just the first: a list that somehow grew two rows for
    // one key heals here.
    let key = entry.dedupe_key();
    entries.retain(|e| e.dedupe_key() != key);

    if entries.iter().any(|e| e.id() == entry.id()) {
        entry.set_id(fresh_id());
    }

    entries.insert(0, entry);
    trim_to(entries, max_count);
}

/// Drops entries past `max_count`, oldest first. A cap of zero empties the list.
/// Answers whether anything was dropped.
pub fn trim_to<E>(entries: &mut Vec<E>, max_count: usize) -> bool {
    let before = entries.len();
    entries.truncate(max_count);
    entries.len() != before
}

/// Removes the entry with this id. Answers whether one was there.
pub fn remove_from<E: RecentItem>(entries: &mut Vec<E>, id: &str) -> bool {
    let before = entries.len();
    entries.retain(|e| e.id() != id);
    entries.len() != before
}

/// Drops every entry whose dedupe key a NEWER one (nearer the top) already holds: what
/// a rewrite that respelled entries in place needs, since it can make two rows one
/// thing. Answers whether anything was dropped.
pub fn dedupe_keeping_newest<E: RecentItem>(entries: &mut Vec<E>) -> bool {
    let before = entries.len();
    let mut seen = std::collections::HashSet::new();
    entries.retain(|e| seen.insert(e.dedupe_key()));
    entries.len() != before
}

#[cfg(test)]
mod tests;
