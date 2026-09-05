# Icons module

OS icon retrieval and caching for the file list. Entries carry only an `iconId`; the frontend batches the unique ids for
visible rows and fetches each once via `get_icons`, so 50k files never transmit 50k icon blobs.

The Rust `src/icons/` module. (`src-tauri/icons/` at the crate root holds the app *bundle* icons, unrelated.)

## Icon-id scheme

`get_icon_id` (in `crates/cmdr-fs/src/entry.rs`, next to `FileEntry`) assigns each entry an id; `get_icons` resolves it to a base64 WebP
data URL. The namespace, by tier:

| Tier | Id | Assigned to | Fetched from |
| --- | --- | --- | --- |
| A | `dir` / `symlink-dir` | every plain folder (~99%) | a temp sample folder |
| A | `ext:{x}` / `file` / `symlink*` | files | a per-extension temp sample / `/etc/hosts` |
| B | `special:{name}` | the finite special system folders | the folder's REAL path (8 MB thread) |
| C | `path:{dir}` / `pkg:{dir}` | per-path icons (volumes, packages, custom-icon folders) | the real path (8 MB thread) |
| n/a | `git:{branch,tag,commit,fork}` | git-portal virtual entries | rendered by the FE via Lucide, never here |

Full details: `DETAILS.md`.

## Must-knows

- **The two per-entry classifiers live in `cmdr-fs`, re-exported here.** `special_folders` and the package half of
  `per_path` run inside `FileEntry::new`, so they had to move where `FileEntry` is; everything expensive (NSWorkspace,
  `getxattr`, the disk cache) stayed. Anything reachable from `get_icon_id` must stay pure and cheap.
- **Special folders are detected by canonical path, NOT by name, with no disk I/O.** `classify` is a lexical `HashMap`
  lookup; never add a `canonicalize` (it blocks on dead mounts and runs per entry during listing).
- **Custom-icon detection (`getxattr`) must NOT run during bulk listing.** A syscall per directory in a 100k-entry
  listing regresses the hot path. It runs only for the bounded set of visible directory paths the FE asks about via
  `get_custom_folder_icon_ids`. Packages (`is_package_dir`, a pure suffix check, no syscall) are the exception and stay
  inline in `get_icon_id`. So a custom-icon folder KEEPS the `dir` id: the FE resolves it by PATH
  (`getCachedCustomFolderIcon`), ❌ never by `iconId`, or the icon is fetched and never drawn.
- **`dir` / `symlink-dir` sample a Cmdr-owned empty temp folder, ❌ never `~`.** macOS bakes the home folder's house
  badge (and any custom icon on `~`) into the bitmap, stamping it onto ~99% of rows.
- **Real-folder NSWorkspace fetches run on dedicated 8 MB-stack OS threads (`fetch_path_icons`), never rayon.** Real
  folders can be cloud folders whose icon lookup descends through deep FileProvider XPC chains that overflow rayon's
  2 MB worker stack. The extension branch (sample temp paths, never cloud) stays on rayon.
- **All NSWorkspace fetches are FDA-gated in `commands/icons.rs`** (they touch TCC services); the FE re-requests after
  the gate clears.
- **Bounded vs unbounded key lifecycle**: `dir` / `ext:*` / `file` / `symlink*` / `special:*` are bounded (uncapped
  in-memory, persisted to localStorage), so changing how one is PRODUCED needs a `CACHE_SCHEMA` bump in
  `$lib/icon-cache` or every existing install serves the old pixels forever. `path:*` / `pkg:*` are unbounded
  (`PATH_KEY_CAP` LRU, never persisted). `pkg:*` shares the `path:*` lifecycle via `is_per_path_key`.
- **A theme/accent change must drop the appearance-tinted keys AND the disk cache.** macOS tints folder glyphs by
  appearance; the mtime token can't catch a system-only change. `clear_directory_icon_cache` handles both.
- **Disk-cache staleness token is the folder's mtime.** Don't replace it with a watcher: Finder bumps the folder mtime
  when re-iconing, which is exactly the invalidation signal.
- **Linux skips NSWorkspace** and resolves via XDG theme lookup, so `special:*` degrades to the generic folder icon.
- **The macOS fetch is ours (`macos_workspace.rs`), not a crate**, because an icon crate can hard-link a framework
  newer than the bundle's macOS floor for code we never call, which is a dyld launch failure rather than a missing
  icon. Drawing rules (per-call bitmap, `Copy` compositing, the canonicalize) in `DETAILS.md`.
