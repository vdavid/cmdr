# Icons module details

Depth and rationale for OS icon retrieval and caching. `CLAUDE.md` holds the must-knows; the tier narratives,
detection-timing decision, and disk-cache mechanism live here.

This is the Rust `src/icons/` module. (`src-tauri/icons/`, a sibling at the crate root, holds the app *bundle* icons,
unrelated.)

## Cache tiers and persistence

`dir` / `ext:*` / `file` / `symlink*` / `special:*` are inherently bounded, so they're uncapped in the in-memory cache
and persist to localStorage on the FE. `path:*` / `pkg:*` are unbounded (grow with folders visited), so they're LRU-
capped (`PATH_KEY_CAP`, 256) and never persisted to localStorage. The Rust side keeps a persistent on-disk warm tier
for the real-folder ids (`special:*` / `pkg:*` / `path:*`), keyed by folder mtime (see § Persistent on-disk cache).

`clear_directory_icon_cache` drops the keys macOS appearance-tints (`dir`, `symlink-dir`, `path:*`, `pkg:*`,
`special:*`) plus the whole disk cache, on a theme/accent change.

## Tier B: special system folders (`special_folders.rs`)

The finite set: Downloads, Desktop, Documents, Movies, Music, Pictures, Public, the home folder, plus (macOS only)
Applications and the Trash. Detected by canonical path, NOT by name: a folder merely named "Downloads" under
`~/Projects/` is not the real one and stays `dir`. The real paths are resolved once at startup via the `dirs` crate
(`/Applications` and `~/.Trash` are hardcoded; `dirs` has no entry for them). `classify` is a lexical-path `HashMap`
lookup with no disk I/O (no `canonicalize`, which would block on a dead mount), so it's cheap per entry during listing.

`get_icons` re-keys each uncached `special:*` id to its real path, fetches via the 8 MB `fetch_path_icons` thread (the
real folder can be iCloud-synced and descend into `fileproviderd`; see `file_system/CLAUDE.md` § Gotchas), then caches
under the bounded `special:{name}` key. The FE renders the fetched icon and falls back to the generic `dir` glyph while
the fetch is pending, FDA-gated, or timed out: purely additive.

Symlinks to a special location keep `symlink-dir` (the link badge is the salient signal; following the link to classify
would cost a syscall per entry).

## Tier C: genuinely per-path icons (`per_path.rs`)

Packages and custom-icon folders, both unbounded by nature, so the expensive NSWorkspace fetch is gated to folders that
actually deviate, detected cheaply. Two signals with deliberately different detection timing:

- **Packages** (`Safari.app`, `Foo.bundle`, …): `is_package_dir` is a pure, no-I/O suffix check on the directory name
  against a bounded extension list (`.app`, `.bundle`, `.framework`, `.plugin`, `.kext`, `.prefpane`, …). Cheap enough
  to run for every entry, so `get_icon_id` routes packages straight to a `pkg:{path}` key during listing. `.app` icons
  are per-app (each distinct), so the key carries the full path; they can't share a bounded `special:`-style key.
- **Custom-icon folders**: the `kHasCustomIcon` flag (`0x0400`) in the folder's `com.apple.FinderInfo` xattr (one
  `getxattr`, no NSWorkspace, no TCC). `has_custom_folder_icon` needs a syscall, so it is NOT run during bulk listing (a
  `getxattr` per directory in a 100k-entry listing would regress the hot path). Instead the FE asks about the bounded
  set of visible directory paths via `get_custom_folder_icon_ids` (→ `icons::custom_folder_icon_ids`), which runs the
  `getxattr` only for those and returns a `path:{dir}` id for each folder that truly has the flag. The
  `finder_info_has_custom_icon` byte-buffer parser is split out pure for testing (flag at offset 8, big-endian `u16`).

**Why the detection split (perf decision)**: the bulk `list_directory` path runs `get_icon_id` per entry. The package
suffix check is free (string op, no syscall), so it stays inline. The custom-icon `getxattr` is a syscall per dir, so
it's deferred to the bounded visible set. Net: a 100k-entry directory pays zero extra syscalls for custom-icon
detection during listing; the cost is bounded to the ~50 visible rows.

**Volumes** carry their own per-path icon through a separate, already-wired path: `volumes/mod.rs` calls
`icons::get_icon_for_path` at volume-enumeration time and stores the data URL directly on the volume struct (FDA-gated,
returns `None` while pending). Independent of the `iconId` registry used for file-list rows, so no Tier-C wiring is
needed for volumes.

`get_icons` treats every real-folder id uniformly: `real_path_for_real_folder_id` maps `special:{name}` → its resolved
location and `pkg:{path}` / `path:{path}` → the embedded path, fetches each via the 8 MB `fetch_path_icons` thread, and
re-keys the result back to the original id. `pkg:*` shares the `path:*` lifecycle: both match `is_per_path_key`, are
LRU-capped together under one `PATH_KEY_CAP` budget, and are never persisted to localStorage.

**FE wiring** (`file-explorer/views/file-list-utils.ts` + `icon-cache.ts`): the visible-range fetch collects the
on-screen directory rows' paths and calls `prefetchCustomFolderIcons` → `get_custom_folder_icon_ids`, then fetches the
returned `path:` ids through the normal `prefetchIcons` path (packages already arrive as `pkg:` ids from the listing).
`FilePane` evicts a directory's `path:*` / `pkg:*` keys via `evictPerPathIconsForDir` when its listing ends (navigation
away / unmount), keeping the working set tight and re-detecting a re-icon next time the folder is shown.

## Persistent on-disk cache (`disk_cache.rs`)

Real-folder icons (`special:*`, `pkg:*`, `path:*`) rarely change, so they persist across restarts in a warm on-disk
tier under `<data_dir>/icon-cache/` (env-resolved via `CMDR_DATA_DIR`, like the secret store). Each entry is a small
JSON sidecar named by an FNV-1a digest of the icon id (so arbitrary path characters never produce an unsafe filename),
holding `{ token, data_url }`.

**Staleness token = the folder's own mtime** (whole epoch seconds). On a hot-cache miss, `get_icons` calls
`disk_cache::load` BEFORE the cold NSWorkspace fetch; a hit promotes the icon into the in-memory LRU. When the user
re-icons a folder in Finder, the folder's mtime bumps (Finder rewrites the icon resource / `com.apple.FinderInfo`), so
the stored token no longer matches and we re-fetch: durability plus correct invalidation without watching anything. A
missing/corrupt sidecar, an unresolvable mtime (dead mount), or any I/O error is a graceful miss; writes are temp+rename
atomic and best-effort.

**Theme/accent change wipes the disk cache too** (`disk_cache::clear_all`, called from `clear_directory_icon_cache`):
macOS tints folder glyphs by appearance, which the mtime token can't catch (the folder didn't change, the system did),
so we drop the warm tier wholesale and let icons re-fetch with the new tint. The tier (in-memory hot LRU → on-disk warm
→ NSWorkspace cold) keeps the common case instant while staying honest about appearance and re-icon changes.

The pure `load_in` / `store_in` (explicit cache dir) underpin the public `load` / `store` (process-wide `CACHE_DIR`),
so tests run hermetically against a temp dir.
