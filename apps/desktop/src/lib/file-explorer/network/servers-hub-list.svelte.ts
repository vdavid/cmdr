/**
 * What one servers hub lists, as live state: the saved servers it read, the rows
 * and items built from them, and whether its nearby group is open.
 *
 * The building itself is pure and lives next door (`servers-hub-rows.ts`,
 * `servers-hub-items.ts`); this is the state those functions are fed from, one
 * instance per hub, so `ServersHub.svelte` stays the table, the cursor, and the
 * keys. Call `createHubList()` during the component's init: it holds runes.
 */

import { listSavedServers, type SavedServer } from '$lib/tauri-commands'
import { getVolumes } from '$lib/stores/volume-store.svelte'
import { getNearbyServersGroupChoice } from '$lib/settings/reactive-settings.svelte'
import { setSetting } from '$lib/settings'
import { getAppLogger } from '$lib/logging/logger'
import { LogOnceGate } from '$lib/logging/log-once'
import { getListedAccount, getNetworkHosts } from './network-store.svelte'
import { buildHubRows, savedSmbHostIds } from './servers-hub-rows'
import { hubItems, isNearbyGroupExpanded, visibleHubItems } from './servers-hub-items'

const log = getAppLogger('servers')

export function createHubList() {
  /** `listSavedServers()`, refreshed whenever the volume list is. */
  let savedServers = $state<SavedServer[]>([])

  /**
   * Whether the person had a saved server when this hub's first read of the
   * saved list answered; `null` until it does.
   *
   * ❗ Two jobs. Until it's known the hub lists NOTHING: which servers are saved
   * decides where every row goes, so listing the hosts first would put them all
   * in the nearby group, open it, and leave the cursor on a row that then moves.
   * And it's what an untoggled nearby group goes by for as long as the view is
   * up, ❌ not the live count: a first save would otherwise fold the group away
   * under the person who is looking at it.
   */
  let hadSavedServerAtOpen = $state<boolean | null>(null)

  /**
   * The nearby group opened for the cursor's sake (an agent's `move_cursor`, a
   * selection after Add) rather than by the person: it stays open for this
   * view, and ❌ isn't remembered as their choice.
   */
  // `as boolean`: only `revealNearby` below sets it, which TypeScript's flow analysis can't see from here.
  let nearbyRevealed = $state(false as boolean)

  const hosts = $derived(getNetworkHosts())
  const volumes = $derived(getVolumes())
  const rows = $derived(
    hadSavedServerAtOpen === null
      ? []
      : buildHubRows({ saved: savedServers, hosts, volumes, listedAs: getListedAccount }),
  )
  const nearbyExpanded = $derived(
    nearbyRevealed || isNearbyGroupExpanded(getNearbyServersGroupChoice(), hadSavedServerAtOpen === true),
  )
  const items = $derived(hubItems(rows, nearbyExpanded))
  const visibleItems = $derived(visibleHubItems(items))

  /** Retried on every `volumes-changed`, so a store that stays broken logs once until a read works. */
  const savedServersReadFailures = new LogOnceGate()

  async function refreshSaved(): Promise<void> {
    try {
      // `Array.isArray` because this is an IPC boundary: a command that
      // answered with nothing would otherwise put `undefined` where the
      // merge iterates, and the hub would render nothing at all.
      const answer: unknown = await listSavedServers()
      savedServers = Array.isArray(answer) ? (answer as SavedServer[]) : []
      savedServersReadFailures.clear()
    } catch (e) {
      // A store that didn't answer costs the hub its saved rows, never the
      // nearby ones: the list is still useful, and the next `volumes-changed`
      // tries again.
      const error = String(e)
      if (savedServersReadFailures.shouldLog(error)) {
        log.warn('Reading the saved servers broke down: {error}', { error })
      }
    }
    hadSavedServerAtOpen ??= savedServers.length > 0
  }

  return {
    get hosts() {
      return hosts
    },
    get volumes() {
      return volumes
    },
    /** Every row, saved servers first. Empty until the saved list is known. */
    get rows() {
      return rows
    },
    /** Every item, the ones a collapsed group hides included: what an agent indexes into. */
    get items() {
      return items
    },
    /** What is on screen, which is what the cursor counts. */
    get visibleItems() {
      return visibleItems
    },
    get nearbyExpanded() {
      return nearbyExpanded
    },
    /** Whether the first read of the saved list answered, so an empty list means "nothing" rather than "not yet". */
    get isKnown() {
      return hadSavedServerAtOpen !== null
    },
    /** The discovered hosts that are the person's own, which are the ones whose shares may be listed unasked. */
    savedHostIds: () => savedSmbHostIds(savedServers, hosts),
    refreshSaved,
    /**
     * Opens or collapses the nearby group, and remembers it: from here on the
     * group opens the way the person left it, whatever they have saved.
     */
    toggleNearbyGroup(): void {
      const expand = !nearbyExpanded
      nearbyRevealed = false
      setSetting('network.nearbyServersGroup', expand ? 'expanded' : 'collapsed')
    },
    /** Opens the nearby group for this view without recording a choice. */
    revealNearby(): void {
      nearbyRevealed = true
    },
  }
}
