/**
 * "Copy share link": three ids, one per expiry, sharing one body.
 *
 * The context menu offers it only on a file a volume can mint a link for, and the
 * palette only while the focused pane is on such a volume, both by capability
 * (`rowCanShareLink`, `paletteCondition`). This still checks the cursor row,
 * because the palette doesn't know whether the cursor is on a file.
 *
 * ❗ The link itself never reaches this side: Rust mints it and writes the
 * clipboard (`copyShareLink`), since its signature is a credential.
 */

import { copyShareLink } from '$lib/tauri-commands'
import type { ShareLinkExpiry } from '$lib/ipc/bindings'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'
import { getFocusedPaneVolumeId } from '$lib/file-explorer/pane/focused-pane-reads'
import { rowCanShareLink } from '$lib/file-explorer/pane/volume-capabilities'
import { renderVolumeError } from '$lib/file-operations/mutation-error-messages'
import type { CommandHandlerContext, CommandHandlerRecord } from './types'

const COPIED: Record<ShareLinkExpiry, MessageKey> = {
  sevenDays: 'commands.handler.shareLinkCopied.sevenDays',
  oneDay: 'commands.handler.shareLinkCopied.oneDay',
  oneHour: 'commands.handler.shareLinkCopied.oneHour',
}

async function copyShareLinkUnderCursor(
  { explorerRef }: CommandHandlerContext,
  expiresIn: ShareLinkExpiry,
): Promise<void> {
  const volumeId = getFocusedPaneVolumeId()
  const row = await explorerRef?.getCursorRowForTerminal()
  if (!row || !rowCanShareLink(volumeId, row)) {
    addToast(tString('commands.handler.shareLinkUnavailable'), { level: 'warn' })
    return
  }
  const outcome = await copyShareLink(volumeId, row.path, expiresIn)
  if (outcome.ok) {
    addToast(tString(COPIED[expiresIn]), { level: 'success', toastGroup: 'share-link', maxInGroup: 1 })
  } else {
    addToast(tString('commands.handler.shareLinkFailed', { detail: renderVolumeError(outcome.error) }), {
      level: 'error',
    })
  }
}

export const shareLinkHandlers = {
  'file.copyShareLink': (hctx) => copyShareLinkUnderCursor(hctx, 'sevenDays'),
  'file.copyShareLinkOneDay': (hctx) => copyShareLinkUnderCursor(hctx, 'oneDay'),
  'file.copyShareLinkOneHour': (hctx) => copyShareLinkUnderCursor(hctx, 'oneHour'),
} satisfies Partial<CommandHandlerRecord>
