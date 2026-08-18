# Go to path details

Depth and rationale. `CLAUDE.md` holds the must-knows; the decision rationale and v1 limitations live here.

## Key decisions

- **The backend owns all path resolution; the frontend is a thin presenter.** One `resolve` call serves three callers:
  the live as-you-type warning, the actual jump, and the clipboard-prefill check. A single resolution path means the
  preview and the action can never drift (AGENTS.md principle 3, "smart backend, thin frontend").
- **Lexical `.`/`..` normalization, never `Path::canonicalize`.** `canonicalize` requires the whole path to exist (it
  errors otherwise), which would break the nearest-ancestor case where the tail doesn't exist. It also resolves
  symlinks, silently rewriting the path we show and navigate to into something the user didn't type. Lexical
  normalization keeps the displayed path faithful and lets nearest-ancestor work. `metadata()` still follows symlinks
  for the file-vs-dir classification, so a symlinked dir navigates into the symlink path and the listing follows it,
  correct and intended.
- **`resolve_go_to_path` is async + `blocking_with_timeout` (2s).** It calls `metadata`/`exists`, which can block
  indefinitely on a hung NFS/SMB mount; the timeout keeps a dead mount from freezing the IPC thread (AGENTS.md
  § Platform constraints).
- **Recents rides on the shared `crate::recents` list.** This module supplies the entry shape, the dedupe key (the
  resolved path string), and the cap; the file, dedupe, eviction, and quarantine are shared with Search and Selection.
  The cap stays a per-call argument, so this store's fixed `MAX_RECENTS` never becomes a knob the tunable lists carry, or
  the reverse. Where the shared type's edge sits, and why: `apps/desktop/src-tauri/src/recents/DETAILS.md`.

## v1 limitations

- **Relative paths on a non-local pane.** `base_dir` is the focused pane's path; if that pane is on MTP/SMB, a relative
  input resolves against a non-local base and the local-fs walk falls back to nearest-ancestor (often `/`). Absolute and
  `~` paths always work. Accepted degraded behavior; don't engineer around it.
- **Case-insensitive dedupe.** The dedupe key is a raw resolved-path string, so on case-insensitive APFS
  `/Users/x/Foo` and `/Users/x/foo` show as two entries. Worst case: a duplicate-looking row. We don't `canonicalize()`
  to fix it (symlink / nearest-ancestor reasons above). Pinned by `the_dedupe_key_is_case_sensitive` in `history.rs`, so
  a change to it is deliberate.
