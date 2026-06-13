# Favorites (backend)

User-editable favorites: the ordered `favorites.json` store that backs the volume switcher's
"Favorites" section. The store is the single source of truth and replaces the previously hardcoded
four favorites. Full depth in [`DETAILS.md`](DETAILS.md).

## Module map

- `store.rs`: the `favorites.json` store. Pure core (`add`/`remove`/`rename`/`reorder` on a `Vec`,
  unit-tested) plus disk I/O, the in-memory cache, and seed-once. Public API: `list`, `add`,
  `remove`, `rename`, `reorder`, and the `Favorite { id, path, name }` type.
- IPC lives in `commands/favorites.rs` (not here): thin `add_favorite` / `remove_favorite` /
  `rename_favorite` / `reorder_favorites` pass-throughs. There's no `list_favorites`; listing rides
  `list_volumes` / `volumes-changed`.

## Must-knows

- **Seed-once via file presence.** File ABSENT means first launch: seed the platform defaults and
  write them. File PRESENT (even an empty list) means already-initialized: read verbatim, NEVER
  re-seed. `read_store_from_path` returns `Option` for exactly this: `None` = absent (seed), `Some`
  = present (don't). A corrupt/version-mismatched file quarantines to `.broken` and reads as
  `Some(empty)`, NOT `None`, so a stray hand-edit can't silently re-seed over a user who'd cleared
  their list. Don't "simplify" the `Option` to a plain `Vec`: it would erase the absent-vs-empty
  distinction the whole contract rests on.
- **`id` is a random UUID minted on add, never derived from `path`.** Paths repeat across renames
  and re-adds, so the id must outlive the path. The switcher's `LocationInfo.id` is `format!("fav-{id}")`.
- **Data dir is resolved WITHOUT an `AppHandle`** (mirrors `install_id.rs`: `CMDR_DATA_DIR` else the
  OS default for `BUNDLE_ID`). Load-bearing: `get_favorites()` (the read path, in `volumes/mod.rs`
  and `volumes_linux/mod.rs`) is sync and `AppHandle`-free, so `store::list()` must stay no-arg. Keep
  `BUNDLE_ID` in sync with `tauri.conf.json`.
- **FDA-pending skip on the read side.** `volumes::get_favorites` must NOT stat a TCC-protected path
  while the FDA gate is pending (even `Path::exists()` trips a TCC popup). It skips the existence
  check for paths where `restricted_paths::tcc_paths::is_potentially_tcc_restricted(path)` is true.
  This now applies to ANY user-added path, not just the old hardcoded three. Linux has no TCC, so its
  twin existence-checks everything.
- **Mutations re-emit `volumes-changed`.** Every command calls `volume_broadcast::emit_volumes_changed()`
  after persisting, so both panes' switchers update live. Don't add a polling path.
- **Lock-poison + log compliance.** Uses `IgnorePoison::lock_ignore_poison()` (not `.lock().unwrap()`)
  and `log::{info,warn}!` with `target: "favorites::store"` (no `println!`). The disk lock is never
  held across an `.await`.
