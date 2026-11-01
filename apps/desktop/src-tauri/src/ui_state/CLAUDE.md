# UI state mirror

What the UI knows, mirrored for the backend. The frontend and the listing and write hosts push in; tools and `cmdr://`
resources read.

- `pane_state.rs`: each pane's tabs, listing window, cursor, and selection.
- `dialog_state.rs`: which soft dialogs are open, plus the known-dialog list.
- `archive_password.rs`: what the encrypted-archive prompt is asking, so `unlock` can answer it.
- `listing_errors.rs`: a ring of recent listing failures.
- `terminal_ops.rs`: a ring of write operations that settled, for `await operation_complete` and `queue`.

## Must-knows

- **These hold state, not policy.** No tool logic and no transport here: anything deciding what a caller may do belongs
  in `tools/`.
- **A pane's listing is a window.** `PaneState` holds a slice starting at `loaded_start`, so convert a global index with
  `checked_sub(loaded_start)` before indexing, or a scrolled pane reads the wrong row.

History, and why each store exists: `DETAILS.md`.
