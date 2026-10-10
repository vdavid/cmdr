/**
 * Volume selection by index / name for a pane — the MCP `select_volume` tool and
 * the palette's volume commands. Lifted out of `DualPaneExplorer`; the component
 * keeps the one-line `export function selectVolumeByName` delegate.
 *
 * Both routes fold onto `navigate({ to: { selectVolume }, source: 'user' })`, so
 * the standard volume-switch mechanics (focus shift, history push, new-tab-on-
 * pinned) apply uniformly. A favorite goes through the shared
 * `navigation/open-favorite.ts`, which enters the volume its row names at its path
 * (a saved place dials from there), or refuses with a worded reason the MCP reply
 * repeats; a real volume opens where `pathForPickedVolume` says (a saved server
 * place on its start folder, anything else at its root); the virtual servers-hub
 * volume isn't in the
 * volumes list, so it's special-cased. The switch arm shifts STORE focus but not DOM focus — re-
 * anchoring the container would drop a Space press during the multi-select-then-
 * delete sequence (regression guard: mtp.spec.ts).
 */

import { tString } from '$lib/intl/messages.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { openFavorite } from '../navigation/open-favorite'
import type { FavoriteRefusal } from '../navigation/favorite-reach'
import type { FavoriteOpenedEvent } from '../navigation/favorites-analytics'
import { pathForPickedVolume } from '../navigation/picked-volume-path'
import type { VolumeInfo } from '../types'
import type { NavigateIntent, NavigateResult } from './navigate'

const log = getAppLogger('fileExplorer')

export interface VolumeSelectionDeps {
  getVolumes: () => VolumeInfo[]
  navigate: (intent: NavigateIntent) => NavigateResult
}

/**
 * What a volume select did. `selected` names the volume the pane was sent to (a
 * favorite's CONTAINING volume) and carries `navigate()`'s result, whose `corrected`
 * says when the switch's destination is final: MCP `select_volume` waits on both to
 * report where the pane came to rest.
 */
export type VolumeSelectOutcome =
  | { kind: 'not-found' }
  | { kind: 'selected'; volumeId: string; navigation: NavigateResult }
  /** A favorite the pane can't go to (an unplugged phone, a forgotten server): the pane stays put. */
  | { kind: 'unreachable-favorite'; refusal: FavoriteRefusal; message: string }

export interface VolumeSelection {
  /**
   * Select a volume by zero-based index into the volumes array. `picked` says which
   * surface picked it when it's a favorite (the analytics payload); a command by default.
   */
  selectVolumeByIndex: (
    pane: 'left' | 'right',
    index: number,
    picked?: FavoriteOpenedEvent,
  ) => Promise<VolumeSelectOutcome>
  /** Select a volume by its stable backend identity (`picked`: as for `selectVolumeByIndex`). */
  selectVolumeById: (
    pane: 'left' | 'right',
    volumeId: string,
    picked?: FavoriteOpenedEvent,
  ) => Promise<VolumeSelectOutcome>
  /** Select a volume by name (MCP `select_volume`). The servers hub is virtual. */
  selectVolumeByName: (pane: 'left' | 'right', name: string) => Promise<VolumeSelectOutcome>
}

export function createVolumeSelection(deps: VolumeSelectionDeps): VolumeSelection {
  function select(pane: 'left' | 'right', volumeId: string, path: string, exact?: boolean): VolumeSelectOutcome {
    const navigation = deps.navigate({
      pane,
      to: { selectVolume: { volumeId, path } },
      source: 'user',
      ...(exact ? { exact } : {}),
    })
    return { kind: 'selected', volumeId, navigation }
  }

  async function selectVolumeByIndex(
    pane: 'left' | 'right',
    index: number,
    picked: FavoriteOpenedEvent = { surface: 'command', via: 'command' },
  ): Promise<VolumeSelectOutcome> {
    const volumes = deps.getVolumes()
    if (index < 0 || index >= volumes.length) {
      log.warn('Invalid volume index: {index} (valid range: 0-{max})', { index, max: volumes.length - 1 })
      return { kind: 'not-found' }
    }

    const volume = volumes[index]

    // A favorite is a row pointing at a path on some other volume, so it takes the
    // shared open (`navigation/open-favorite.ts`), which reads the volume off the row
    // and emits. One the pane can't go to says why, in words the MCP reply repeats.
    if (volume.category === 'favorite') {
      const opened = await openFavorite({
        favorite: volume,
        pane,
        picked,
        go: (target) => select(pane, target.volumeId, target.targetPath, target.exact),
      })
      return opened.kind === 'opened'
        ? opened.opened
        : { kind: 'unreachable-favorite', refusal: opened.refusal, message: opened.message }
    }
    // A saved server place opens on its start folder; anything else at its root.
    return select(pane, volume.id, pathForPickedVolume(volume))
  }

  async function selectVolumeById(
    pane: 'left' | 'right',
    volumeId: string,
    picked?: FavoriteOpenedEvent,
  ): Promise<VolumeSelectOutcome> {
    if (volumeId === 'network') return select(pane, 'network', 'smb://')

    const index = deps.getVolumes().findIndex((v) => v.id === volumeId)
    if (index !== -1) return selectVolumeByIndex(pane, index, picked)

    log.warn('Volume not found: {volumeId}', { volumeId })
    return { kind: 'not-found' }
  }

  async function selectVolumeByName(pane: 'left' | 'right', name: string): Promise<VolumeSelectOutcome> {
    // ❗ The servers hub row is SYNTHETIC: `volume-grouping.ts` builds it, so it
    // is not in the volume list and no `findIndex` can reach it. Its name comes
    // from the catalog rather than a literal, so the label, the MCP pane push,
    // and Rust's `volume_listing::SERVERS_VOLUME_NAME` stay one word.
    if (name === tString('fileExplorer.navigation.networkVolume')) {
      return select(pane, 'network', 'smb://')
    }

    const matches = deps.getVolumes().flatMap((volume, index) => (volume.name === name ? [index] : []))
    if (matches.length === 1) {
      return selectVolumeByIndex(pane, matches[0])
    }
    if (matches.length > 1) {
      log.warn('Volume name is ambiguous: {volumeName}', { volumeName: name })
      return { kind: 'not-found' }
    }

    log.warn('Volume not found: {volumeName}', { volumeName: name })
    return { kind: 'not-found' }
  }

  return { selectVolumeByIndex, selectVolumeById, selectVolumeByName }
}
