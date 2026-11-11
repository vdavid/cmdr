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

- **The two per-entry classifiers live in `cmdr-fs`, re-exported here** (`special_folders`, the package half of
  `per_path`): they run inside `FileEntry::new`, so anything reachable from `get_icon_id` must stay pure and cheap.
- **Special folders are detected by canonical path, NOT by name, with no disk I/O**: `classify` is a lexical `HashMap`
  lookup. Never add a `canonicalize`: it blocks on dead mounts, per entry, during listing.
- **Custom-icon detection (`getxattr`) must NOT run during bulk listing** (a syscall per directory): only for the
  visible paths the FE asks about via `get_custom_folder_icon_ids`. So a custom-icon folder KEEPS the `dir` id: the FE
  resolves it by PATH (`getCachedCustomFolderIcon`), ❌ never `iconId`, or the icon is fetched and never drawn.
- **Gold recolors a custom icon only if `followsAccent`** (Customize Folder symbol/emoji, no colored tag); an emoji
  folder is gold `dir` plus the emoji on top. `DETAILS.md`.
- **`dir` / `symlink-dir` sample a Cmdr-owned empty temp folder, ❌ never `~`.** macOS bakes the home folder's house
  badge (and any custom icon on `~`) into the bitmap, stamping it onto ~99% of rows.
- **Real-folder NSWorkspace fetches run on 8 MB-stack OS threads (`fetch_path_icons`), never rayon**: a cloud folder's
  FileProvider XPC chain overflows rayon's 2 MB stack. Extension samples (never cloud) stay on rayon.
- **All NSWorkspace fetches are FDA-gated in `commands/icons.rs`**; the FE re-requests once the gate clears.
- **Bounded vs unbounded key lifecycle**: `dir` / `ext:*` / `file` / `symlink*` / `special:*` are bounded (uncapped
  in-memory, persisted to localStorage), so changing how one is PRODUCED needs a `CACHE_SCHEMA` bump in
  `#lib/icon-cache`, or every existing install serves the old pixels forever. `path:*` / `pkg:*` are unbounded
  (`PATH_KEY_CAP` LRU, never persisted), one lifecycle via `is_per_path_key`.
- **System-only changes need explicit flushes**, since the mtime token can't see them: a theme/accent change drops the
  folder keys and the disk tier (`clear_directory_icon_cache`); an `IconVariant` change (Increase contrast, Invert
  colors redraw file AND folder icons) drops all memory keys, and the disk tier keys by it.
- **Every cache write is epoch-guarded** (`cache_icon(epoch, ..)`, FE `applyIconsToCache(.., epoch)`), epoch taken
  BEFORE asking macOS, or a fetch outliving a flush restores the old look. `DETAILS.md` § Flushes.
- **Disk-cache staleness token is the folder's mtime**, ❌ not a watcher: Finder bumps it when re-iconing.
- **Providers live in `platform/`**; off macOS `special:*` degrades to the generic folder.
- **The macOS fetch is ours (`platform/macos/workspace.rs`), not a crate**: a crate can hard-link a framework newer than our
  floor. `DETAILS.md`.
