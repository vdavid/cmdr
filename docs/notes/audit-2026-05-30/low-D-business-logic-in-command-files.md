# Business logic leaks into command files, violating the thin-IPC contract (3 sites)

**Severity:** low
**Lens:** D — IPC boundary (architecture contract)
**Confidence:** high

## Location
- `apps/desktop/src-tauri/src/commands/rename.rs:217-441`
- `apps/desktop/src-tauri/src/commands/search.rs:189-222`
- `apps/desktop/src-tauri/src/commands/file_system/write_ops.rs:378-418`

## What
AGENTS.md mandates a thin IPC layer ("Tauri commands are pass-throughs: no branching, no transformation. Business logic lives in subsystem modules"), and commands/CLAUDE.md repeats it. Three command files carry non-trivial logic instead:

1. **`rename.rs`** implements an entire rename-validity subsystem inline: `check_rename_permission_sync`, `check_dir_writable` (raw `libc::access`), `check_macos_flags` (raw `libc::lstat` + UF/SF_IMMUTABLE bit logic), `check_rename_validity_impl` (filename + path-length validation + branching), `check_sibling_conflict` (inode `dev()/ino()` comparison for case-only renames), and `check_sibling_conflict_via_volume` — ~225 lines.
2. **`search.rs`** post-filters `search()` results inside the command: a `retain` loop applying `min_size`/`max_size` directory filtering, recomputing `total_count`, and truncating to limit.
3. **`write_ops.rs`** (`emit_synthetic_entry_diff`) stats a new entry, enriches it with index data, finds affected listings, inserts sorted, and enqueues diffs — a four-step interaction with the listing-cache/diff subsystem.

## Why it matters
This logic is testable on its own and currently can't be unit-tested without a Tauri runtime. It also drifts from the architecture the rest of the codebase follows, so a future maintainer reading these files learns the wrong pattern. Not a correctness/safety bug — purely an architecture-contract violation, but one the audit was explicitly asked to surface (lens D #1).

## Evidence
```rust
// rename.rs:361 — inode comparison for case-only rename, in the command file
fn check_sibling_conflict(...) -> (bool, bool, Option<ConflictFileInfo>) { /* dev()/ino() */ }
// search.rs:194 — result-shaping in the command
result.entries.retain(|e| { /* min/max size dir filter */ });
result.total_count = result.entries.len() as u32;   // :215
```

## Suggested fix
Move the helpers into their subsystem modules: rename validity → `crate::file_system::rename` (a `validation` module already exists next door); search post-filter → fold into `search::search` or a `search::finalize_results` helper; `emit_synthetic_entry_diff` → `file_system::listing` (e.g. `listing::emit_synthetic_create_diff`). Leave the `#[tauri::command]` fns as timeout-wrapped delegators.

## Notes
The rest of the IPC surface is thin and clean (network/git/clipboard/settings/file_viewer commands reviewed and fine). Filed as a single low finding rather than three to avoid fragmenting one architectural drift.
