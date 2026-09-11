/**
 * Where a pane goes when the user picks a volume ITSELF: a switcher row, a hub
 * row, the palette's and MCP's volume select, and a server row's Open.
 *
 * A pane headed somewhere specific (a restored tab, a favorite, go-to-path, a
 * history walk) never comes through here, so it keeps its path.
 *
 * Its own module, importing nothing but a type, so the switcher, the hub, and
 * the volume-selection factory can share it without pulling in the path probes.
 */

import type { VolumeInfo } from '../types'

/**
 * The path a picked volume opens at.
 *
 * ❗ A `saved` place opens straight on its landing (its start folder), because
 * the CONNECT is what lands it: nothing is registered yet, so neither the other
 * pane nor the remembered path can be probed, and the pane waits right there
 * while it dials.
 *
 * ❗ Every other volume opens at its ROOT, which `determineNavigationPath` reads
 * as "pick the best path": the other pane, then the remembered path, then the
 * landing. ❌ Handing it the landing instead reads as a favorite and skips both.
 */
export function pathForPickedVolume(volume: VolumeInfo): string {
  if (volume.connectionState === 'saved') return volume.landingPath ?? volume.path
  return volume.path
}
