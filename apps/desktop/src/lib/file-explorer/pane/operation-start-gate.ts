/**
 * The one place a command that would START a file operation is refused, and the one
 * place that refusal is worded.
 *
 * Two gates share it, because they catch different things:
 *
 * - **The entry points** (`file-operation-commands.ts`): a dialog is on screen, so
 *   a confirmation would stack over what the user is reading. Which dialogs count
 *   is declared per dialog in `$lib/ui/dialog-registry.ts`.
 * - **The start itself** (`dialog-state.svelte.ts`): the progress slot is taken.
 *   That one has to stand alone whatever the entry points do — the native menu is
 *   OS-side and MCP is a separate actor, and neither passes through this window's
 *   modal state.
 *
 * ❌ Neither gate touches the commands that STEER a running operation. Cancel,
 * pause, resume, rollback, queue, and answering a name clash all keep working with
 * the progress dialog up, which is exactly when a user reaches for them. Search's
 * own "Show all in main window" is navigation, so it keeps working too.
 *
 * The full scope and the reasoning: `DETAILS.md` § "The operation-start gate".
 */

import { emit } from '@tauri-apps/api/event'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { blockingSoftDialog } from '$lib/ui/open-dialogs.svelte'
import type { SoftDialogId } from '$lib/ui/dialog-registry'

const log = getAppLogger('fileExplorer')

/**
 * The refusal an agent reads. It says what's in the way and what clears it, so a
 * capable agent recovers in one step instead of retrying into the same wall or
 * waiting out a round-trip timeout.
 *
 * ⚠️ The identity travels as the typed `blockedBy` field beside this sentence,
 * never only inside it: the repo's `no-error-string-match` rule applies to a
 * message an agent parses just as it does to one our own code would. Which is
 * also why the sentence stops at "close it": naming the tool and repeating the id
 * would be a second copy of what `blockedBy` already carries, free to drift.
 */
export function mcpOperationBlockedMessage(blockedBy: SoftDialogId): string {
  return `The ${blockedBy} dialog is open, so nothing new can start. Close it first, then try again.`
}

/**
 * Says the operation isn't starting, to whoever asked: a toast for the person who
 * picked File > Copy, and a failed round-trip for an MCP agent, which beats making
 * it wait out the round-trip budget for silence.
 */
export function announceOperationBlocked(blockedBy: SoftDialogId, mcpRequestId: string | undefined): void {
  log.info('Not starting an operation: the {blockedBy} dialog is in the way', { blockedBy })
  addToast(tString('fileOperations.transferProgress.operationBlockedToast'), { level: 'info' })
  if (mcpRequestId) {
    void emit('mcp-response', {
      requestId: mcpRequestId,
      ok: false,
      blockedBy,
      error: mcpOperationBlockedMessage(blockedBy),
    })
  }
}

/**
 * The entry-point gate: refuses and announces when a dialog is on screen, and
 * returns whether the caller should stop.
 *
 * The MCP tools are refused in Rust before they ever reach the frontend
 * (`mcp/executor/mod.rs::refuse_while_dialog_blocks`), so in practice this catches
 * the native menu, whose items stay clickable whatever is on screen.
 */
export function operationStartIsBlocked(mcpRequestId?: string): boolean {
  const blockedBy = blockingSoftDialog()
  if (!blockedBy) return false
  announceOperationBlocked(blockedBy, mcpRequestId)
  return true
}
