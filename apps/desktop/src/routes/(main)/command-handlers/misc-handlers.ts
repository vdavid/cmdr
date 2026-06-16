/**
 * Singletons that don't belong to a larger family: go-to-latest-download, the
 * network-host refresh, and the per-pane MCP `select_volume`. Each is a lone
 * action with no shared helper, so folding any one into nav / pane would scatter
 * unrelated logic rather than improve cohesion. Kept together as the residual
 * bucket; the dispatch core stays small either way.
 *
 * `downloads.goToLatest` AWAITS `goToLatestDownload` (a follow-the-download
 * navigation); preserve its `await`.
 */
import { goToLatestDownload } from '$lib/downloads/go-to-latest'
import { addFavorite } from '$lib/tauri-commands'
import { getFocusedPanePath } from '$lib/file-explorer/pane/focused-pane-reads'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import type { CommandArgs } from '$lib/commands'
import type { CommandHandlerRecord } from './types'

/** The last path segment, for a friendly toast label (`/Users/me/Docs` → `Docs`). */
function lastSegment(path: string): string {
  const trimmed = path.replace(/\/+$/, '')
  const slash = trimmed.lastIndexOf('/')
  return slash >= 0 ? trimmed.slice(slash + 1) || trimmed : trimmed
}

export const miscHandlers = {
  'downloads.goToLatest': async ({ explorerRef }) => {
    await goToLatestDownload(explorerRef)
  },

  'favorites.add': async () => {
    // Favorites the focused pane's current folder. The context-menu paths
    // (folder row, `..`) favorite a specific path in Rust instead, so this
    // handler only covers the palette / menu / shortcut surface.
    const path = getFocusedPanePath()
    if (!path) return
    try {
      await addFavorite(path, null)
      addToast(tString('commands.handler.favoriteAdded', { name: lastSegment(path) }), { level: 'success' })
    } catch {
      addToast(tString('commands.handler.favoriteAddFailed'), { level: 'error' })
    }
  },

  'network.refresh': ({ explorerRef }) => {
    explorerRef?.refreshNetworkHosts()
  },

  'volume.selectByName': ({ explorerRef, dispatchArgs }) => {
    // MCP `select_volume` tool: select a SPECIFIC pane's volume by name.
    // `selectVolumeByName` drives the `navigate()` transaction for the switch.
    const { pane, name } = dispatchArgs as CommandArgs['volume.selectByName']
    void explorerRef?.selectVolumeByName(pane, name)
  },
} satisfies Partial<CommandHandlerRecord>
