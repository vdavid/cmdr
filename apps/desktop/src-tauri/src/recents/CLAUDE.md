# Recents

One persisted, deduped, capped list, shared by the three dialogs that keep recents: Search ("Recent searches"),
Selection ("Recent selections"), and Go to path ("Recent paths"). A consumer supplies the entry type and what makes two
entries the same; this module owns everything else.

## Module map

- **`mod.rs`**: the `RecentEntry` trait (filename, log identity, id, dedupe key, schema version), `RecentsFile<E>` (the
  in-memory list + its two locks + the app-bound facade), and the pure list ops (`add_to`, `trim_to`, `remove_from`).
- **`persistence.rs`**: the `_schemaVersion` envelope, the durable temp+rename write, and the set-aside path for a file
  we can't read.

## Must-knows

- **Declare one list per consumer as a `static`** (`RecentsFile::new()` is `const`) and drive it from that feature's IPC
  module. The three live in `search/history.rs`, `selection/history.rs`, and `go_to_path/history.rs`.
- **The cap is a per-call argument, never a property of the list.** Search and Selection read theirs from a live
  setting; Go to path passes a const. Baking a cap into the store is what would couple the three.
- **Only most-recently-used lists belong here**: `add` puts an entry on top. A list in the user's own order
  (Multi-rename's presets) has its own store, `../multi_rename/preset_store.rs`.
- **The two locks are never held at once, by construction.** `update` confines the cache guard to its own body and hands
  the disk write an owned snapshot; `load_at` releases the disk guard before taking the cache one. Keep any new
  operation inside `update` and the property holds itself.
- **A file we can't read is set aside, not migrated.** A read error, a parse failure (non-UTF-8 included), or an
  unknown `_schemaVersion` goes through `state_recovery::read_state_file` (a timestamped copy, the person told once,
  keyed by `RecentEntry::STATE_FILE`) and the list starts empty, so a stray edit can't break a dialog forever, and the
  next add can't write over the old list. When a v2 lands,
  the version check in `persistence::read` becomes a `match`.
- **The disk write is best-effort.** A failure is logged and the in-memory list stays consistent, so losing the app data
  dir costs persistence and never the session.

Rationale, the entry-shape boundary, and what deliberately stayed per-feature: `DETAILS.md`.
