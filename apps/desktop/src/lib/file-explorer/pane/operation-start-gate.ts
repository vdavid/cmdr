/**
 * The vocabulary of the operation-start gate: what we say to an MCP agent whose
 * copy, move, compress, delete, or create was refused because a dialog is up.
 *
 * The dialog's identity travels as a typed `blockedBy` field alongside this
 * sentence, never inside it. The sentence is for a human reading the agent's
 * transcript; the field is what the agent acts on.
 *
 * Which dialogs block and why: `$lib/ui/dialog-registry.ts`. The gate's scope
 * (starting an operation, never steering a running one): `DETAILS.md` § "The
 * operation-start gate".
 */

import type { SoftDialogId } from '$lib/ui/dialog-registry'

/**
 * The refusal an agent reads. It names the dialog and the one action that clears
 * it, so a capable agent recovers in one step instead of retrying into the same
 * wall or waiting out a round-trip timeout.
 */
export function mcpOperationBlockedMessage(blockedBy: SoftDialogId): string {
  return `The ${blockedBy} dialog is open, so nothing new can start. Close it first (the dialog tool's close action, id ${blockedBy}), then try again.`
}
