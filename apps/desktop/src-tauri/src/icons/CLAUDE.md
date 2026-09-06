# Icons module

OS icon retrieval and caching for the file list. Entries carry only an `iconId`; the FE batches the unique ids for
visible rows and fetches each once via `get_icons`, so 50k files never transmit 50k icon blobs. (`src-tauri/icons/` at
the crate root holds the app *bundle* icons, unrelated.)

## Icon-id scheme

`get_icon_id` (`crates/cmdr-fs/src/entry.rs`, next to `FileEntry`) assigns each entry an id; `get_icons` resolves it to
a base64 WebP data URL. By tier:

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
  `per_path` run inside `FileEntry::new`; everything expensive (NSWorkspace, `getxattr`, the disk cache) stayed here.
  Anything reachable from `get_icon_id` must stay pure and cheap.
- **Special folders are detected by canonical path, NOT by name, with no disk I/O**: `classify` is a lexical `HashMap`
  lookup. Never add a `canonicalize`: it blocks on dead mounts, per entry, during listing.
- **Custom-icon detection (`getxattr`) must NOT run during bulk listing**: a syscall per directory regresses a
  100k-entry listing. It runs only for the visible directory paths the FE asks about via `get_custom_folder_icon_ids`;
  packages (`is_package_dir`, a pure suffix check) stay inline. So a custom-icon folder KEEPS the `dir` id: the FE
  resolves it by PATH (`getCachedCustomFolderIcon`), ❌ never `iconId`, or the icon is fetched and never drawn.
- **`dir` / `symlink-dir` sample a Cmdr-owned empty temp folder, ❌ never `~`.** macOS bakes the home folder's house
  badge (and any custom icon on `~`) into the bitmap, stamping it onto ~99% of rows.
- **Real-folder NSWorkspace fetches run on dedicated 8 MB-stack OS threads (`fetch_path_icons`), never rayon**: a cloud
  folder's icon lookup descends through FileProvider XPC chains deep enough to overflow rayon's 2 MB worker stack. The
  extension branch (temp samples, never cloud) stays on rayon.
- **All NSWorkspace fetches are FDA-gated in `commands/icons.rs`** (TCC services); the FE re-requests once the gate
  clears.
- **Bounded vs unbounded key lifecycle**: `dir` / `ext:*` / `file` / `symlink*` / `special:*` are bounded (uncapped
  in-memory, persisted to localStorage), so changing how one is PRODUCED needs a `CACHE_SCHEMA` bump in
  `$lib/icon-cache`, or every existing install serves the old pixels forever. `path:*` / `pkg:*` are unbounded
  (`PATH_KEY_CAP` LRU, never persisted), one lifecycle via `is_per_path_key`.
- **A theme/accent change must drop the appearance-tinted keys AND the disk cache**: macOS tints folder glyphs by
  appearance, and the mtime token can't catch a system-only change. `clear_directory_icon_cache` handles both.
- **Disk-cache staleness token is the folder's mtime.** Don't replace it with a watcher: Finder bumps the mtime when
  re-iconing, which is exactly the invalidation signal.
- **Linux skips NSWorkspace** and resolves via XDG theme lookup, so `special:*` degrades to the generic folder icon.
- **The macOS fetch is ours (`macos_workspace.rs`), not a crate**, because an icon crate can hard-link a framework
  newer than the bundle's macOS floor for code we never call: a dyld launch failure, not a missing icon. Drawing rules
  in `DETAILS.md`.
