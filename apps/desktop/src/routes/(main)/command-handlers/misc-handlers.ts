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
import type { CommandArgs } from '$lib/commands'
import type { CommandHandlerRecord } from './types'

export const miscHandlers = {
  'downloads.goToLatest': async ({ explorerRef }) => {
    await goToLatestDownload(explorerRef)
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
