# Favorites (backend)

User-editable favorites: the ordered `favorites.json` store behind the favorites menu (⌃D), and what
each favorite row tells the frontend about where it points. Full depth in `DETAILS.md`.

## Module map

- `store.rs`: the `favorites.json` store (`Favorite { id, path, name, shortcut, volume }`): pure
  mutations (tested in `store_tests.rs`), disk I/O, the cache, seed-once, `claim_volumes`.
- `target.rs`: what a favorite row publishes (`FavoriteTarget`, `FavoriteReach`) and discovery's
  side of it (`on_disk`). `reach.rs`: the listing's reach pass, rebase, and legacy claims.
- IPC lives in `commands/favorites.rs`: the pass-throughs plus THE add gate. No `list_favorites`;
  listing rides `list_volumes` / `volumes-changed`.

## Must-knows

- **Seed-once via file presence.** ABSENT = first launch: seed and write. PRESENT (even empty) =
  read verbatim, never re-seed. A corrupt or version-mismatched file quarantines and reads as
  `Some(empty)`, ❌ never `None`. Don't "simplify" `read_store_from_path`'s `Option` to a `Vec`: the
  absent-vs-empty distinction is the whole contract.
- **No schema bump for additive fields**: a mismatch QUARANTINES, so a downgrader would lose the
  whole list. `volume` is serde-defaulted, like `shortcut`.
- **Every add goes through `commands::favorites::add_favorite`, ❌ never `store::add`.** Only the
  gate names the volume a favorite lives on (registered, containing the path, an exhaustively
  admitted backend) and saves an unsaved SMB share, unpinned. `DETAILS.md` § The add gate.
- **A favorite's identity is its volume id + the path under its root**, like a tab's. The reach
  pass decides `FavoriteReach` from rows alone, and ❌ a favorite row never carries
  `connection_state` (volume-shaped consumers read it). `target.rs` header.
- **The read side never touches a network path, and never hides a favorite.** Discovery stats a
  folder only on a local disk, never a TCC-protected one while the FDA gate is pending, and
  publishes a row for every stored favorite. `DETAILS.md` § The read side.
- **❗ A legacy claim needs evidence**: the boot volume claims a folder only when discovery SAW it,
  or an unmounted share's favorite is written down as the boot disk's forever. Claims fill `None`
  only and persist off the listing path. `DETAILS.md` § Claiming legacy entries.
- **`id` is a random UUID, never derived from `path`**; the row's id is `fav-{id}`.
- **Data dir resolves WITHOUT an `AppHandle`** (`config::standalone_app_data_dir()`, a scratch dir
  under `cfg(test)`): discovery reads the store from a sync, handle-less path, so `store::list()`
  stays no-arg.
- **Mutations re-emit `volumes-changed`** after persisting (claims excepted: the row already shows
  them). Don't add a polling path.
