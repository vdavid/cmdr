# Icons module

OS icon retrieval and caching for the file list. Entries carry only an `iconId`; the frontend batches the unique ids
for visible rows and fetches each once via `get_icons`, so 50k files never transmit 50k icon blobs.

This is the Rust `src/icons/` module. (`src-tauri/icons/`, a sibling at the crate root, holds the app *bundle* icons —
unrelated.)

## Icon-id scheme

`get_icon_id` (in `file_system/listing/metadata.rs`) assigns each entry an id; `get_icons` resolves it to a base64 WebP
data URL. The id namespace, by tier:

| Tier | Id | Assigned to | Fetched from |
| --- | --- | --- | --- |
| A | `dir` / `symlink-dir` | every plain folder (~99%) | the home dir (sample) |
| A | `ext:{x}` / `file` / `symlink*` | files | a per-extension temp sample / `/etc/hosts` |
| B | `special:{name}` | the finite special **system** folders | the folder's REAL path (8 MB thread) |
| C | `path:{dir}` / `pkg:{dir}` | per-path icons (volumes, packages, custom-icon folders) | the real path (8 MB thread) |
| — | `git:{branch,tag,commit,fork}` | git-portal virtual entries | rendered by the FE via Lucide, never here |

`dir` / `ext:*` / `file` / `symlink*` / `special:*` are inherently **bounded**, so they're uncapped and persist to
localStorage. `path:*` / `pkg:*` are **unbounded** (grow with folders visited), so they're LRU-capped (`PATH_KEY_CAP`)
and never persisted. See `clear_directory_icon_cache` for which keys a theme/accent change drops (`dir`, `symlink-dir`,
`path:*`, `special:*` — all appearance-tinted by macOS).

## Tier B — special system folders (`special_folders.rs`)

The finite set: Downloads, Desktop, Documents, Movies, Music, Pictures, Public, the home folder, plus (macOS only)
Applications and the Trash. Detected by **canonical path**, NOT by name — a folder merely *named* "Downloads" under
`~/Projects/` is not the real one and stays `dir`. The set of real paths is resolved once at startup via the `dirs`
crate (`/Applications` and `~/.Trash` are hardcoded; `dirs` has no entry for them). `classify` is a lexical-path
`HashMap` lookup with **no disk I/O** (no `canonicalize` — it would block on a dead mount), so it's cheap enough to run
per entry during listing.

`get_icons` re-keys each uncached `special:*` id to its real path, fetches via the 8 MB `fetch_path_icons` thread (the
real folder can be iCloud-synced and descend into `fileproviderd`; see `file_system/CLAUDE.md` § Gotchas), then caches
under the bounded `special:{name}` key. The FE renders the fetched icon and falls back to the generic `dir` glyph while
the fetch is pending, FDA-gated, or timed out — the feature is purely additive.

Symlinks to a special location keep `symlink-dir` (the link badge is the salient signal; following the link to classify
would cost a syscall per entry).

## Threading + FDA

Per-path / per-special NSWorkspace fetches run on dedicated 8 MB-stack OS threads (`fetch_path_icons`), never rayon —
real folders can be cloud folders whose icon lookup descends through deep FileProvider XPC chains that overflow rayon's
2 MB worker stack. The extension branch (sample temp paths, never cloud) stays on rayon. All fetches are FDA-gated in
`commands/icons.rs` (NSWorkspace touches TCC services); the FE re-requests after the gate clears. Linux skips
NSWorkspace entirely and resolves via the XDG theme lookup, so `special:*` degrades to the generic folder icon there.
