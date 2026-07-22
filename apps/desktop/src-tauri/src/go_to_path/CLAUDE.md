# Go to path (backend)

Backend for the "Go to path" dialog (⌘G): the user types a path and jumps to it in the focused pane. Owns all path
reasoning plus the recent-paths store; the IPC layer (`commands/go_to_path.rs`) is a thin pass-through.

## Module map

- **`mod.rs`**: pure `resolve(input, base_dir) -> GoToPathResolution` (`~` expansion, relative-to-`base_dir` join,
  lexical `.`/`..` normalization, nearest-ancestor walk, dir/file classify) + unit tests
- **`history.rs`**: recent-paths store (`RecentPathEntry`, `RecentPathsStore`): in-memory `Mutex` + `OnceLock`, atomic
  temp+rename write, dedupe by resolved path, fixed cap, schema-version quarantine

Decision rationale and v1 limitations: `DETAILS.md`.

## Must-knows

- **`GoToPathResolution` is the contract; never classify by string-matching the `reason` message** (AGENTS.md
  § no-error-string-match). Four variants: `Directory { path }`, `File { path, parentDir, fileName }`,
  `NearestAncestor { requested, ancestorDir }`, `Invalid { reason }`. The `File` variant carries the canonical
  normalized full `path` (the frontend records it into recents verbatim, no client-side reconstruction) plus `parentDir`
  / `fileName` (drive navigate-to-parent + select). The tagged-enum serde attrs (`tag = "kind"`,
  `rename_all = "camelCase"`, `rename_all_fields = "camelCase"`) are required so the tag and struct-variant fields ship
  camelCase through tauri-specta (enforced by `ipc-enum-camelcase`).
- **Lexical `.`/`..` normalization, never `Path::canonicalize`.** `canonicalize` requires the whole path to exist (it
  errors otherwise), breaking the nearest-ancestor case, and it resolves symlinks, silently rewriting the path shown and
  navigated to. Lexical keeps the displayed path faithful to the input. `metadata()` still follows symlinks for the
  file-vs-dir classify, so a symlinked dir navigates into the symlink path and the listing follows it (intended).
- **`resolve_go_to_path` is async + `blocking_with_timeout` (2s)** because `metadata`/`exists` can block on a hung
  NFS/SMB mount (AGENTS.md § Platform constraints).
- **Recents store keys on the resolved target, not the raw input.** Entry `{ id, timestamp, path }` where `path` is the
  dir, file path, or nearest ancestor we actually jumped to. Dedupe by resolved-path string with move-to-top. Fixed cap
  is a const `MAX_RECENTS = 10` (not a setting); the dialog shows up to 10 via digit keys 1-9, 0. File
  `go-to-path-history.json` in the app data dir; schema-versioned, a parse error or version mismatch quarantines to
  `.broken` and starts fresh. Populated only by manual dialog jumps (frontend convention, not enforced here).
- **Lock-poison: keep the `search/history.rs` idiom verbatim** (`.lock().unwrap_or_else(|e| e.into_inner())` and
  `match … Err(poisoned) => poisoned.into_inner()`). "Simplifying" to `.lock().unwrap()` trips the `lock-poison` check.
