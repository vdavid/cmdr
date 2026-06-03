# Go to path (backend)

Backend for the "Go to path" dialog (⌘G): the user types a path and jumps to it in the focused pane. This module owns
all the path reasoning plus the recent-paths store. The IPC layer (`commands/go_to_path.rs`) is a thin pass-through.

## Module structure

| File         | Purpose                                                                                                                                                        |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `mod.rs`     | Pure `resolve(input, base_dir) -> GoToPathResolution`: `~` expansion, relative-to-`base_dir` join, lexical `.`/`..` normalization, nearest-ancestor walk, dir/file classify. Plus unit tests. |
| `history.rs` | Recent-paths store (`RecentPathEntry`, `RecentPathsStore`): in-memory `Mutex` + `OnceLock`, atomic temp+rename write, dedupe by resolved path, fixed cap, schema-version quarantine. |

## `GoToPathResolution`

The resolution outcome the frontend branches on. Four variants: `Directory { path }`,
`File { path, parentDir, fileName }`, `NearestAncestor { requested, ancestorDir }`, `Invalid { reason }`. The `File`
variant carries the canonical normalized full `path` (the frontend records it into recents verbatim, no client-side
reconstruction) alongside `parentDir` / `fileName` (which drive navigate-to-parent + select). The tagged-enum attrs
(`#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]`) are required so the tag and the
struct-variant fields ship camelCase through tauri-specta (enforced by `ipc-enum-camelcase`).

The variant is the contract; never classify by string-matching the `reason` message (AGENTS.md § no-error-string-match).

## Recents store

- Entry: `{ id, timestamp, path }`. `path` is the **resolved target we actually jumped to** (a dir, the file path, or
  the nearest ancestor), never the raw typed input.
- Dedupe key = the resolved-path string. Move-to-top on re-add. Fixed cap **10** (a const, `MAX_RECENTS`, not a
  setting): the dialog shows at most 10 recents via digit keys 1-9, 0.
- File: `go-to-path-history.json` in the app data dir. Schema-versioned; a parse error or version mismatch quarantines
  the file to `.broken` and starts fresh.
- Populated only by manual jumps in the dialog (frontend convention, not enforced here), matching the search-history
  "record only on the explicit action" precedent.

## Key decisions

**Decision**: The backend owns all path resolution; the frontend is a thin presenter.
**Why**: One `resolve` call serves three callers - the live as-you-type warning, the actual jump, and the
clipboard-prefill check. A single resolution path means the preview and the action can never drift (AGENTS.md principle
3, "smart backend, thin frontend").

**Decision**: Lexical `.`/`..` normalization, never `Path::canonicalize`.
**Why**: `canonicalize` requires the whole path to exist (it errors otherwise), which would break the nearest-ancestor
case where the tail doesn't exist. It also resolves symlinks, silently rewriting the path we show and navigate to into
something the user didn't type. Lexical normalization keeps the displayed path faithful to the input and lets
nearest-ancestor work. `metadata()` still follows symlinks for the file-vs-dir classification, so a symlinked dir
navigates into the symlink path and the listing follows it - correct and intended.

**Decision**: `resolve_go_to_path` is async + `blocking_with_timeout` (2s).
**Why**: It calls `metadata`/`exists`, which can block indefinitely on a hung NFS/SMB mount. The timeout keeps a dead
mount from freezing the IPC thread (AGENTS.md § Platform constraints).

**Decision**: Recents store is a clone-and-trim of `search/history.rs`, not a shared generic.
**Why**: The two stores share a _shape_, not a contract (search has tunable cap, modes, filters; this has a fixed cap
and a single `path` field). A premature generic would couple search's knobs to this store's fixed cap. Clone the lines
that matter (elegance lives between duplication and overengineering - AGENTS.md).

## v1 limitations

- **Relative paths on a non-local pane.** `base_dir` is the focused pane's path; if that pane is on MTP/SMB, a relative
  input resolves against a non-local base and the local-fs walk falls back to nearest-ancestor (often `/`). Absolute and
  `~` paths always work. Accepted degraded behavior - don't engineer around it.
- **Case-insensitive dedupe.** The recents store dedupes by a raw resolved-path string compare, so on case-insensitive
  APFS `/Users/x/Foo` and `/Users/x/foo` show as two entries. Worst case: a duplicate-looking row. We don't
  `canonicalize()` to fix it (symlink / nearest-ancestor reasons above).

## Lock-poison compliance

The store clones `search/history.rs`'s lock idiom verbatim: `.lock().unwrap_or_else(|e| e.into_inner())` and
`match … Err(poisoned) => poisoned.into_inner()`. A "simplification" to `.lock().unwrap()` trips the `lock-poison`
check.
