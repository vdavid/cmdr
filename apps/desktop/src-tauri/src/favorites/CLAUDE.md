# Favorites (backend)

User-editable favorites: the ordered `favorites.json` store that backs the frontend's favorites
menu (⌃D). The store is the single source of truth and replaces the previously hardcoded four
favorites. Full depth in `DETAILS.md`.

## Module map

- `store.rs`: the `favorites.json` store. Pure mutations on a `Vec` (tested in `store_tests.rs`),
  disk I/O, the in-memory cache, and seed-once. Public API: `list`, `add`, `remove`, `rename`,
  `reorder`, `set_shortcut`, `claim_volumes`, and `Favorite { id, path, name, shortcut, volume }`.
- IPC lives in `commands/favorites.rs` (not here): `add_favorite` / `remove_favorite` /
  `rename_favorite` / `reorder_favorites` / `set_favorite_shortcut` pass-throughs, plus THE add gate. There's no
  `list_favorites`; listing rides `list_volumes` / `volumes-changed`.

## Must-knows

- **Seed-once via file presence.** File ABSENT means first launch: seed the platform defaults and
  write them. File PRESENT (even an empty list) means already-initialized: read verbatim, NEVER
  re-seed. `read_store_from_path` returns `Option` for exactly this: `None` = absent (seed), `Some`
  = present (don't). A corrupt/version-mismatched file quarantines to `.broken` and reads as
  `Some(empty)`, NOT `None`, so a stray hand-edit can't silently re-seed over a user who'd cleared
  their list. Don't "simplify" the `Option` to a plain `Vec`: it would erase the absent-vs-empty
  distinction the whole contract rests on.
- **Every add goes through `commands::favorites::add_favorite`, ❌ never `store::add`.** That's the
  only place the add gate runs, and the gate isn't optional: `volumes::get_favorites` HIDES a
  favorite whose path isn't on disk, so an ungated add writes an entry no menu can ever show.
  The native folder-row menu calls the command for exactly this reason. What it accepts and why:
  `DETAILS.md` § The add gate.
- **`id` is a random UUID minted on add, never derived from `path`.** Paths repeat across renames
  and re-adds, so the id must outlive the path. The frontend's `LocationInfo.id` is `format!("fav-{id}")`.
- **Data dir is resolved WITHOUT an `AppHandle`**, via `config::standalone_app_data_dir()` (a
  per-process scratch dir under `cfg(test)`, so `list_locations()` in a test never seeds the real
  file). Load-bearing: `get_favorites()` (the read path, in `volumes/mod.rs` and
  `volumes_linux/mod.rs`) is sync and `AppHandle`-free, so `store::list()` must stay no-arg.
- **FDA-pending skip on the read side.** `volumes::get_favorites` must NOT stat a TCC-protected path
  while the FDA gate is pending (even `Path::exists()` trips a TCC popup). It skips the existence
  check for paths where `restricted_paths::tcc_paths::is_potentially_tcc_restricted(path)` is true.
  This now applies to ANY user-added path, not just the old hardcoded three. Linux has no TCC, so its
  twin existence-checks everything.
- **Mutations re-emit `volumes-changed`.** Every command calls `volume_broadcast::emit_volumes_changed()`
  after persisting, so both panes' menus update live. Don't add a polling path.
- **Lock-poison + log compliance.** Uses `IgnorePoison::lock_ignore_poison()` (not `.lock().unwrap()`)
  and `log::{info,warn}!` with `target: "favorites::store"` (no `println!`). The disk lock is never
  held across an `.await`.
