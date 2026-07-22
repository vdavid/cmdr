# Favorites (backend) details

User-editable favorites. The volume switcher's "Favorites" section is fully user-owned: add, remove,
rename, reorder. This module owns the ordered `favorites.json` store; the IPC layer
(`commands/favorites.rs`) is a thin pass-through. Read `CLAUDE.md` first for the
must-knows.

## What it replaces

Favorites used to be a hardcoded `Vec<LocationInfo>` of four folders, computed fresh on every
`get_favorites()` call with no write path. Now `get_favorites()` (both the macOS `volumes/mod.rs` and
the Linux `volumes_linux/mod.rs` twins) reads `favorites::store::list()` and maps each entry to a
`LocationInfo` with `category: Favorite`. The frontend already tells favorites apart from volumes via
`category`, so no extra "user-removable" flag is needed: every favorite is now a user favorite.

## On-disk shape

`favorites.json` in the app data dir:

```json
{
  "_schemaVersion": 1,
  "favorites": [
    { "id": "9f1c…", "path": "/Applications", "name": "Applications" },
    { "id": "a83e…", "path": "/Users/me/Desktop", "name": "Desktop" }
  ]
}
```

- `id`: a random UUID minted on add, never derived from `path`. The switcher exposes it as
  `LocationInfo.id = "fav-<id>"`.
- `path`: the absolute filesystem path.
- `name`: the display label. Defaults to the path's file name on add; the user can override via
  rename.

Order in the array is the display order.

## Seed-once contract

`favorites.json` existing means "already initialized." Four states:

- **File absent** (`read_store_from_path` returns `None`): first launch. Seed the platform defaults
  and write them. This is the only path that ever writes defaults.
- **File present, non-empty**: read verbatim.
- **File present, empty list**: the user cleared every favorite. Read verbatim (stays empty). Never
  re-seed.
- **File present, corrupt or wrong `_schemaVersion`**: quarantine to `<name>.broken`, then read as
  `Some(empty)` (NOT `None`). A broken file is "initialized but unreadable," so we must not re-seed
  the defaults over a user who had intentionally cleared their list.

Existing beta users (data dir present, no `favorites.json` yet) hit the absent branch on the first
launch after the update and get the platform defaults seeded, so there's no regression from the old
hardcoded behavior.

Platform defaults (`default_favorites`, platform-native per `design-principles.md`):

- macOS: `/Applications`, `~/Desktop`, `~/Documents`, `~/Downloads` (the previous hardcoded four).
- Linux: Home, `~/Desktop`, `~/Documents`, `~/Downloads` (matching the old `volumes_linux`
  favorites).

## Operations (pure core)

All in `store.rs`, unit-tested without disk or an `AppHandle`:

- `add(path, name?)`: dedups by normalized path. A re-add moves the existing entry to the end and
  keeps its id (applying a `name` override if given). A fresh add appends with a UUID id and a label
  defaulting to the path's file name. Move-to-end (not move-to-top) because favorites are a curated,
  ordered list, not a recency stack: a re-add shouldn't reshuffle the user's deliberate ordering more
  than necessary.
- `remove(id)`: drops by id. No-op if absent.
- `rename(id, name)`: updates the label by id. No-op if absent.
- `reorder(ordered_ids)`: reorders to match the given id list. Unknown ids are ignored; favorites
  whose ids are missing from the list are appended in their current relative order, so a partial or
  stale order from the frontend never drops an entry.

`normalize_for_dedup` strips a single trailing `/` (but keeps root `/`). Case-sensitivity is a known
limitation, same as `go_to_path/history.rs`: on case-insensitive APFS `/Users/x/Foo` and
`/Users/x/foo` compare unequal (worst case: a duplicate-looking row). We don't `canonicalize()` (it
would resolve symlinks and require the path to exist).

## Persistence and concurrency

- In-memory cache: `OnceLock<Mutex<Option<FavoritesStore>>>`. `None` until first access loads (and
  lazily seeds) from disk.
- `DISK_LOCK` (a separate `Mutex<()>`) serializes the read-modify-write cycle so concurrent commands
  can't clobber each other. `load_or_seed` re-checks the cache under the disk lock so two concurrent
  first-access callers can't both seed.
- Atomic writes via `config::durable_write_json` (write-tmp + fsync + rename + parent-dir fsync),
  the same data-loss-class discipline the rest of the app uses.
- The disk lock is never held across an `.await`; the in-memory guard is always dropped before any
  `fs` call. (The commands themselves run the store calls inside `spawn_blocking`, so even the
  synchronous store API never blocks the IPC thread.)

## IPC contract (`commands/favorites.rs`)

Thin async pass-throughs, each `blocking_result_with_timeout` (5s, the write tier) since the store
write touches the filesystem. After persisting, each re-emits `volumes-changed` via
`volume_broadcast::emit_volumes_changed()` so both panes' switchers refresh live
(subscribe-don't-poll). Listing rides the existing `list_volumes` / `volumes-changed` path, so
there's no `list_favorites` command.

- `add_favorite(path: String, name: Option<String>) -> Result<(), IpcError>`
- `remove_favorite(id: String) -> Result<(), IpcError>`
- `rename_favorite(id: String, name: String) -> Result<(), IpcError>`
- `reorder_favorites(ordered_ids: Vec<String>) -> Result<(), IpcError>`

Registered in both `ipc.rs` (runtime `generate_handler`) and `ipc_collectors.rs` (specta types).

## FDA-pending skip (macOS)

`volumes::get_favorites` must not stat a TCC-protected path while the FDA gate is pending: even
`Path::exists()` trips a macOS TCC popup for the protected-folder service once the bundle is
registered with tccd, which is exactly the onboarding-flood the FDA modal exists to prevent. The read
maps each favorite, skipping the existence check when the FDA gate is pending AND
`restricted_paths::tcc_paths::is_potentially_tcc_restricted(path)` is true (and assuming such a
protected favorite exists). Non-protected paths are still checked (for example `/Applications` can be
absent on slim systems). This now applies to ANY user-added path, not just the old hardcoded three.
Linux has no TCC, so its twin existence-checks everything and there's no gate.

## Local-filesystem paths only (v1)

Favorites are local filesystem paths for now. Network and MTP favorites are deferred (mount-state
complexity). The store doesn't enforce this; the add surfaces in the frontend gate it.

## MCP consumer

The MCP `favorites` tool wraps the `commands::favorites` pass-throughs (add / rename / remove / reorder), and
`cmdr://state` `favorites:` reads `store::list()` for id discovery. See `mcp/DETAILS.md`.
