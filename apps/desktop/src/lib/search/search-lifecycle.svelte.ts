/**
 * The Search-only lifecycle around a dialog session, and the readiness questions it
 * answers while the session lasts.
 *
 * On mount: subscribe to `search-index-ready`, ask the backend to pre-load root's arena,
 * load the persisted recent searches, and fetch the system-dir exclude list for the
 * tooltip. On destroy: release the index (naming the one run a pane is still being fed
 * by) and drop the listener.
 *
 * In between it owns the gate: **"is the volume this run will land on worth waiting
 * for?"**, not "is root loaded". The rationale for each of the gate's three inputs, and
 * the one known gap (a root pre-load that fails to read its DB emits nothing, so the
 * dialog stays on "Loading index…" for the session), are in `DETAILS.md` § The readiness
 * gate is per target.
 *
 * Selection has none of this: no whole-drive index, so no gate and no lifecycle.
 */

import {
  prepareSearchIndex,
  releaseSearchIndex,
  getSystemDirExcludes,
  onSearchIndexReady,
  type UnlistenFn,
} from '$lib/tauri-commands'
import { isVolumeScanning, getEntriesScanned, ROOT_VOLUME_ID } from '$lib/indexing'
import { tString } from '$lib/intl/messages.svelte'
import { isTargetIndexReady } from './coverage-note'
import { loadRecentSearches } from './recent-searches-state.svelte'
import { buildSystemDirExcludeTooltip } from './system-dir-tooltip'
import {
  getDateFilter,
  getIsIndexAvailable,
  getMode,
  getPendingIndexVolumeId,
  getQuery,
  getScope,
  getSizeFilter,
  getVolumeEntryCount,
  isVolumeIndexReady,
  markVolumeIndexReady,
  searchQueryState,
  setIsIndexAvailable,
  setPendingIndexVolumeId,
} from './search-state.svelte'

export interface SearchLifecycleDeps {
  /** The ONE volume this session covers: the focused pane's current volume. */
  getSearchVolumeId: () => string
  /**
   * The run this dialog handed to a pane, or `null`. Read at teardown, because the close
   * must spare exactly that run: `release_search_index` cancels every run BUT the one it
   * names, and a `null` here kills the walk the instant the pane appears.
   */
  getHandedOffRunId: () => string | null
}

export interface SearchLifecycle {
  /** QueryDialog's `onMount` hook. */
  setup: () => Promise<void>
  /** QueryDialog's `onDestroy` hook. */
  teardown: () => void
  /** The volume this run will land on, or `null` when only the backend can say. */
  readonly targetVolumeId: string | null
  /** Whether a search may run now. */
  readonly isIndexReady: boolean
  /** The target's own indexed entry count: 0 (so the empty state stays quiet) until its arena lands. */
  readonly indexEntryCount: number
  /** Whether the backend can answer at all (a failed `prepare` turns this off). */
  readonly isIndexAvailable: boolean
  /** Whether the LOCAL index is still being built, and how far it has got. */
  readonly scanning: boolean
  readonly entriesScanned: number
  /** The "hide boring folders" tooltip; the default until the exclude list arrives. */
  readonly systemDirExcludeTooltip: string
}

export function createSearchLifecycle(deps: SearchLifecycleDeps): SearchLifecycle {
  // Index-readiness listener cleanup. Search-specific (Selection has no whole-drive index).
  let unlistenReady: UnlistenFn | undefined

  // System-dir exclude tooltip (populated async on mount; renders the full exclude list).
  let systemDirExcludeTooltip = $state(tString('search.systemDirExclude.default'))

  /**
   * An unset scope resolves to the focused pane's folder, which is on the pane's own
   * volume — that's the default and the common case. A scope the user typed or clicked
   * in can point anywhere, and routing a path to a volume is the backend's job (an SMB
   * id keys on the address; cloud drives route to `root`), so the honest answer here is
   * "unknown", which `isTargetIndexReady` reads as "run it".
   */
  const targetVolumeId = $derived(getScope().trim() === '' ? deps.getSearchVolumeId() : null)

  /**
   * PER TARGET: waiting is only right while a pre-load for THIS volume is in flight.
   * Gating on root instead left a machine with no root index unable to search at all,
   * since no `search-index-ready` was ever coming.
   */
  const isIndexReady = $derived(
    isTargetIndexReady({
      targetVolumeId,
      isVolumeReady: isVolumeIndexReady,
      pendingVolumeId: getPendingIndexVolumeId(),
    }),
  )
  const indexEntryCount = $derived(targetVolumeId === null ? 0 : getVolumeEntryCount(targetVolumeId))
  const isIndexAvailable = $derived(getIsIndexAvailable())
  // Search reads the LOCAL index, so its "building index" state keys on `root`
  // only — a network (SMB/MTP) scan must not flip the label while root's
  // `entriesScanned` stays 0.
  const scanning = $derived(isVolumeScanning(ROOT_VOLUME_ID))
  const entriesScanned = $derived(getEntriesScanned())

  async function setup(): Promise<void> {
    // Listen for a volume's arena landing. The event NAMES its volume, so readiness
    // is recorded per volume and only the search that targets that one un-gates.
    unlistenReady = await onSearchIndexReady((volumeId: string, entryCount: number) => {
      markVolumeIndexReady(volumeId, entryCount)
      if (getPendingIndexVolumeId() === volumeId) setPendingIndexVolumeId(null)
      // Auto-run the pending search if the user already typed something AND this is
      // the volume they're searching (filename/regex only; AI mode always waits for
      // an explicit Enter / ⌘Enter).
      const pendingMode = getMode()
      if (
        volumeId === targetVolumeId &&
        pendingMode !== 'ai' &&
        (getQuery().trim() || getSizeFilter() !== 'any' || getDateFilter() !== 'any')
      ) {
        // Trigger via the runOnMount flag; QueryDialog's effect dispatches to
        // the non-AI runner since mode !== 'ai'.
        searchQueryState.setRunOnMount(true)
      }
    })

    try {
      // Root is the one volume that gets pre-loaded when the dialog opens. `loading`
      // is the backend's promise that an event is coming; without it, a machine with
      // no root index would wait for one that never arrives and never search at all.
      const result = await prepareSearchIndex()
      if (result.ready) {
        markVolumeIndexReady(ROOT_VOLUME_ID, result.entryCount)
        setPendingIndexVolumeId(null)
      } else {
        setPendingIndexVolumeId(result.loading ? ROOT_VOLUME_ID : null)
      }
    } catch {
      // Index not available: indexing disabled, not started, or backend unavailable.
      setPendingIndexVolumeId(null)
      setIsIndexAvailable(false)
    }

    // Persisted recent searches load (idempotent across the session).
    void loadRecentSearches()

    getSystemDirExcludes()
      .then((dirs) => {
        systemDirExcludeTooltip = buildSystemDirExcludeTooltip(dirs, tString('search.systemDirExclude.heading'))
      })
      .catch(() => {})
  }

  function teardown(): void {
    // A handed-off walk is the one run the close must NOT stop: its results are in
    // a pane and still growing. Every other run in flight is a query nobody is
    // reading any more.
    releaseSearchIndex(deps.getHandedOffRunId()).catch(() => {})
    unlistenReady?.()
    unlistenReady = undefined
  }

  return {
    setup,
    teardown,
    get targetVolumeId() {
      return targetVolumeId
    },
    get isIndexReady() {
      return isIndexReady
    },
    get indexEntryCount() {
      return indexEntryCount
    },
    get isIndexAvailable() {
      return isIndexAvailable
    },
    get scanning() {
      return scanning
    },
    get entriesScanned() {
      return entriesScanned
    },
    get systemDirExcludeTooltip() {
      return systemDirExcludeTooltip
    },
  }
}
