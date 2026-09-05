# Icons module details

Depth and rationale for OS icon retrieval and caching. `CLAUDE.md` holds the must-knows; the tier narratives,
detection-timing decision, and disk-cache mechanism live here.

This is the Rust `src/icons/` module. (`src-tauri/icons/`, a sibling at the crate root, holds the app *bundle* icons,
unrelated.)

## Asking macOS for the pixels (`macos_workspace.rs`)

`NSWorkspace.iconForFile:` answers an `NSImage`, a resolution-independent recipe rather than a buffer, so every fetch draws it: allocate an `NSBitmapImageRep` at the target size, wrap it in an `NSGraphicsContext`, draw, and read `bitmapData`. The bitmap is `RGBA8`, non-planar and tightly packed, which is exactly what `image::RgbaImage::from_raw` adopts with no copy of its own.

- **Per-call bitmap and context, never a shared one.** The fetch runs on rayon workers and on the dedicated 8 MB-stack threads, so one drawing surface would be two threads compositing into the same buffer. The allocation is noise next to the Launch Services round trip.
- **`NSCompositingOperation::Copy`, not `SourceOver`.** The bitmap arrives as uninitialized memory, so blending over it mixes garbage into every transparent pixel. `renders_a_real_icon_at_the_requested_size` pins this on the corner alpha.
- **The path is canonicalized first**, which doubles as the existence check (`iconForFile:` hands back a plausible generic document icon for a path that isn't there, and that would cache under an id meaning something else) and resolves symlinks. The id scheme wants that: `symlink-file` / `symlink-dir` are separate ids fetched from their own samples.

Decision/Why: this replaced the `file_icon_provider` crate, which did the same drawing but also reached `UTType` from a caching struct Cmdr never constructed. That reference alone hard-linked `UniformTypeIdentifiers.framework` into the binary, and since that framework arrived in macOS 11, dyld refused to launch Cmdr at all on the 10.15 floor `tauri.conf.json` promises. Sixty lines of `NSWorkspace` we own beats a dependency that can put a framework in the binary for code we don't run, and it drops `gio`, `gtk`, and `windows` from the macOS graph as a side effect. `desktop-macos-framework-floor` now fails the build on any framework newer than the floor; the version evidence is in `docs/notes/system-requirements-and-es2025.md`.

## Cache tiers and persistence

`dir` / `ext:*` / `file` / `symlink*` / `special:*` are inherently bounded, so they're uncapped in the in-memory cache
and persist to localStorage on the FE. `path:*` / `pkg:*` are unbounded (grow with folders visited), so they're LRU-
capped (`PATH_KEY_CAP`, 256) and never persisted to localStorage. The Rust side keeps a persistent on-disk warm tier
for the real-folder ids (`special:*` / `pkg:*` / `path:*`), keyed by folder mtime (see § Persistent on-disk cache).

`clear_directory_icon_cache` drops the keys macOS appearance-tints (`dir`, `symlink-dir`, `path:*`, `pkg:*`,
`special:*`) plus the whole disk cache, on a theme/accent change.

## Tier A: extension samples (`icon_sample_path`)

An `ext:{x}` icon comes from asking the OS about a real file, so Cmdr keeps one empty stand-in per extension. Only the
suffix matters, and the file stays empty on purpose so nothing content-sniffs it into a different icon.

**Decision: samples live in `<temp>/cmdr-icon-samples/sample.{ext}`, not loose in the temp root.** A bare
`<temp>/cmdr_icon_sample.{ext}` is a fixed name any other process can occupy, and it strands one file per extension ever
seen in the temp root forever. A directory keeps them namespaced and removable as a unit. Reuse across runs is
deliberate: re-creating an empty file every launch buys nothing. Pinned by
`extension_samples_live_in_a_cmdr_owned_directory`.

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

**An icon that was never fetched from the OS** still reaches the frontend in the same base64 WebP form:
`rgba_to_data_url(rgba, w, h)` encodes a raw buffer at `ICON_SIZE`. Its one caller is the "open terminal here" app list
(`../file_system/terminal.rs`), which reads each app's `.icns` straight out of its bundle rather than asking NSWorkspace,
so it needs no TCC permission and can't descend into a FileProvider XPC chain. Nothing here caches it: that list is a
handful of apps, rendered when the settings row opens. It's `#[cfg(target_os = "macos")]` for the same reason its caller
is: `.app` bundles are a macOS thing, and an ungated copy is dead code in the Linux build.

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
