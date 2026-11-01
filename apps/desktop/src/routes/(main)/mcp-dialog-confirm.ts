/**
 * MCP `dialog confirm`: press the open dialog's confirm, or with
 * `startInBackground` its Background button (F2), and answer the agent with the
 * operation that press started.
 *
 * Every confirm is a round-trip (`tools/actions/dialogs.rs`). Its id rides the
 * press into the start, and a press that got there is answered by the START
 * (`transfer-dispatch.ts::startTransferOperation`) with the spawned
 * `operationId`, or by the start gate with what's in the way, exactly as an
 * auto-confirmed `copy` is. Only a press that didn't get there answers here: a
 * refusal crosses as the typed `refusal` field, ❌ never as text.
 */

import type { ConfirmDialogType } from '#lib/commands/index.js'
import { getAppLogger } from '#lib/logging/logger.js'
import type { ExplorerAPI } from './explorer-api'

const log = getAppLogger('mcpListeners')

export async function confirmDialogForMcp(args: {
  explorer: ExplorerAPI | undefined
  type: ConfirmDialogType
  onConflict?: string
  startInBackground?: boolean
  /** The MCP round-trip id. */
  requestId: string | undefined
}): Promise<void> {
  const { explorer, type, onConflict, startInBackground, requestId } = args
  const reply = async (body: Record<string, unknown>): Promise<void> => {
    if (requestId === undefined) return
    const { emit } = await import('@tauri-apps/api/event')
    await emit('mcp-response', { requestId, ...body })
  }

  if (!explorer) {
    log.warn('mcp-confirm-dialog dropped: no explorer is mounted ({type})', { type })
    await reply({ ok: false, error: 'Explorer is not ready' })
    return
  }
  const verdict = await explorer.confirmDialog(type, onConflict, { startInBackground, mcpRequestId: requestId })
  if (verdict.pressed) return
  if (verdict.refusal === 'notReady') {
    await reply({ ok: false, error: `No ${type} dialog is open and ready to confirm` })
  } else {
    await reply({ ok: false, refusal: verdict.refusal })
  }
}
