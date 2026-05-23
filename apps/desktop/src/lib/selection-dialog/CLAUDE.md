# Selection dialog

Stub. The "Select files…" / "Deselect files…" dialog ships in M7 of the
[selection-dialog plan](../../../../../docs/specs/selection-dialog-plan.md). This file gets fully populated then. M6
(pane-side `applyIndices` plumbing) is in place.

## M6 note: snapshot panes

For `search-results://` panes, the Selection dialog's matcher runs against `entry.name`, which on snapshot panes IS the
displayed friendly path (home folder shown as `~`, mid-truncated for display), NOT the raw `entry.path`. See
[`$lib/file-explorer/CLAUDE.md`](../file-explorer/CLAUDE.md) § "Search-results virtual volume" for why `name` carries
the friendly path on adapted entries.

`applyIndices` operates on indices into the snapshot's `entries[]` exactly as for regular panes — no special-casing at
the pane API layer. The dialog passes the right name accessor (per the plan's § "Match semantics") and the matcher
returns indices that the pane treats uniformly.
