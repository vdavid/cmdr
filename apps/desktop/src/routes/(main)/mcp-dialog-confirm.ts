/**
 * MCP `dialog confirm`: press the open dialog's confirm, or with
 * `startInBackground` its Background button (F2), and for a background press
 * tell the agent whether it happened.
 *
 * Only the background press is a round-trip (`mcp/executor/dialogs.rs`): it can
 * be refused, and a refused press leaves the dialog up, which the backend's
 * "dialog went away" ack can't tell from a slow one. A permanent delete refuses
 * it, and the refusal crosses as the typed `refusal` field, ❌ never as text.
 */

import type { ConfirmDialogType } from '$lib/commands'
import { getAppLogger } from '$lib/logging/logger'
import type { ExplorerAPI } from './explorer-api'

const log = getAppLogger('mcpListeners')

export async function confirmDialogForMcp(args: {
  explorer: ExplorerAPI | undefined
  type: ConfirmDialogType
  onConflict?: string
  startInBackground?: boolean
  /** The MCP round-trip id. Present only for a background confirm. */
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
  const verdict = explorer.confirmDialog(type, onConflict, { startInBackground })
  if (verdict.pressed) {
    await reply({ ok: true })
  } else if (verdict.refusal === 'permanentDelete') {
    await reply({ ok: false, refusal: 'permanentDelete' })
  } else {
    await reply({ ok: false, error: `No ${type} dialog is open and ready to confirm` })
  }
}
