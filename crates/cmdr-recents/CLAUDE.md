# cmdr-recents

The rules of a most-recently-used list (newest first, one row per distinct thing, a cap from the oldest end, unique
ids), as pure `Vec` work. Every recents list in Cmdr keeps them: the desktop's search, selection, Go to path, and
multi-rename histories (persisted by `apps/desktop/src-tauri/src/recents/`), and a browser host's session list.

## Module map

- **lib.rs**: the whole crate. `RecentItem` (id, `set_id`, dedupe key) and `add_to`, `trim_to`, `remove_from`,
  `dedupe_keeping_newest`. Tests in `tests.rs`.

## Must-knows

- **No I/O, no clock, no RNG.** A colliding id is replaced with one the host mints (`add_to`'s `fresh_id`), because a
  browser panics on `SystemTime::now`. ❌ No dependency that reads either.
- **The file, its locks, the cap's source, and the quarantine are the host's**, ❌ never here: the desktop's
  `RecentsFile` owns them.
- **The cap is a per-call argument**, ❌ never a property of a list: one list's cap is a live setting, another's a
  const.
- **A zero cap means no list at all**: `add_to` empties it and drops the new entry too.
- The public surface is capped by `index-crate-isolation`, and no `tauri` is allowed in the tree.

Why its own crate: `DETAILS.md`.
