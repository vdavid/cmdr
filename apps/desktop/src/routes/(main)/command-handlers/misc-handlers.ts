/**
 * Singletons that don't belong to a larger family: go-to-latest-download, the
 * network-host refresh, and the per-pane MCP `select_volume`. Each is a lone
 * action with no shared helper, so folding any one into nav / pane would scatter
 * unrelated logic rather than improve cohesion. Kept together as the residual
 * bucket; the dispatch core stays small either way.
 *
 * `downloads.goToLatest` AWAITS `goToLatestDownload` (a follow-the-download
 * navigation); preserve its `await`. `file.goToTrash` awaits for the same reason:
 * it resolves the volume's trash over IPC before any pane moves.
 */
import { goToLatestDownload } from '$lib/downloads/go-to-latest'
import { goToTrash } from '$lib/file-operations/delete/go-to-trash'
import { addFavoriteFolder } from '$lib/file-explorer/navigation/add-favorite-folder'
import { getFocusedPanePath } from '$lib/file-explorer/pane/focused-pane-reads'
import type { CommandArgs } from '$lib/commands'
import { selectVolumeForMcp } from '../mcp-volume-select'
import { detached } from './detached'
import type { CommandHandlerRecord } from './types'

export const miscHandlers = {
  'downloads.goToLatest': async ({ explorerRef }) => {
    await goToLatestDownload(explorerRef)
  },

  'file.goToTrash': async ({ explorerRef }) => {
    await goToTrash(explorerRef)
  },

  'favorites.add': async () => {
    // Favorites the focused pane's current folder. The context-menu paths
    // (folder row, `..`) favorite a specific path in Rust instead, so this
    // handler only covers the palette / menu / shortcut surface.
    const path = getFocusedPanePath()
    if (!path) return
    await addFavoriteFolder(path)
  },

  'favorites.open': ({ explorerRef }) => {
    explorerRef?.toggleFavoritesMenu()
  },

  'network.refresh': ({ explorerRef }) => {
    explorerRef?.refreshNetworkHosts()
  },

  'volume.selectByName': ({ explorerRef, dispatchArgs }) => {
    // MCP `select_volume` tool: select a SPECIFIC pane's volume by id or name, and with a
    // request id reply once the pane has come to rest. Voided on purpose: the landing
    // wait can run for seconds, and nothing downstream of the dispatch reads it.
    const { pane, name, volumeId, mcpRequestId } = dispatchArgs as CommandArgs['volume.selectByName']
    detached(selectVolumeForMcp({ explorer: explorerRef, pane, name, volumeId, requestId: mcpRequestId }))
  },
} satisfies Partial<CommandHandlerRecord>
