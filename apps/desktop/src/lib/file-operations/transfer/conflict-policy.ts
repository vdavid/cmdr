/**
 * The one map from an MCP `onConflict` name to the transfer dialog's own
 * conflict policy.
 *
 * Two surfaces start a transfer programmatically — `copy` / `move` with
 * `autoConfirm` (the dialog confirms itself on mount, `TransferDialog.svelte`)
 * and `dialog confirm` on an already-open one (`dialog-state.svelte.ts`) — and
 * they each used to carry their own copy of this map. The copies had drifted:
 * one spelled the conditional policies `overwrite_all_smaller`, the other
 * `overwrite_smaller_all`, and neither spelling was reachable, so nobody found
 * out. A name the backend accepts and the map doesn't know silently becomes
 * `skip`, which turns "ask me about each file" into "skip every file" against
 * somebody's data.
 *
 * The backend validates the name (`mcp/executor/dialogs.rs`, `file_ops.rs`), so
 * this map is the second half of one contract: keep the two lists together.
 */

import type { ConflictResolution } from '$lib/file-explorer/types'

/**
 * Every policy the MCP tools accept. `stop` is the one that decides nothing: the
 * operation asks per file and parks on each clash until somebody answers it (a
 * person, or an agent through `resolve_conflict`).
 */
const POLICY_BY_MCP_NAME: Partial<Record<string, ConflictResolution>> = {
  stop: 'stop',
  skip_all: 'skip',
  overwrite_all: 'overwrite',
  rename_all: 'rename',
  overwrite_smaller_all: 'overwrite_smaller',
  overwrite_older_all: 'overwrite_older',
}

/** The policy `name` means, or `undefined` when this map has never heard of it. */
export function conflictPolicyFromMcpName(name: string | undefined): ConflictResolution | undefined {
  return name === undefined ? undefined : POLICY_BY_MCP_NAME[name]
}
