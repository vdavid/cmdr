/**
 * Shared harness for `navigate.*.test.ts`: fakes the store reads/writes with a
 * REAL `TabManager` per pane (so the pinned-fork tab splice + the commit are
 * observable on real tab state), a real volume map, and spies for the FilePane
 * handle, the resolver, persistence, and focus.
 *
 * No tests of its own — scaffolding, imported by `navigate.commit.test.ts`,
 * `navigate.arms.test.ts`, and `navigate.refusals.test.ts`, the same way
 * `drag-drop-controller.test-fixtures.ts` backs its siblings.
 */
import { vi } from 'vitest'
import type { NavigateDeps, PersistEvent, LastUsedPathRecord } from './navigate'
import { createTabManager, getActiveTab, type TabManager } from '../tabs/tab-state-manager.svelte'
import { createInitialTabState } from './tab-operations'
import type { FilePaneAPI } from './types'
import type { DetermineNavigationPathArgs } from '../navigation/path-navigation'

/** A live volume map the fake deps resolve paths/names against (a Map so misses are `undefined`). */
const VOLUMES = new Map<string, { id: string; name: string; path: string }>([
  ['root', { id: 'root', name: 'Macintosh HD', path: '/' }],
  ['ext', { id: 'ext', name: 'Ext', path: '/Volumes/Ext' }],
  ['network', { id: 'network', name: 'Network', path: 'smb://' }],
  [
    'sftp-nas-local-22-ada',
    { id: 'sftp-nas-local-22-ada', name: 'Naspolya', path: 'sftp://ada@nas.local:22/srv/data' },
  ],
])

/** A FilePane stub: every method a no-op, `navigateToPath` returns a resolvable promise we can track. */
function makePaneRefStub(): {
  ref: FilePaneAPI
  navigateToPath: ReturnType<typeof vi.fn>
  setNetworkHost: ReturnType<typeof vi.fn>
  navigateToParent: ReturnType<typeof vi.fn>
} {
  const navigateToPath = vi.fn().mockResolvedValue(undefined)
  const navigateToParent = vi.fn().mockResolvedValue(true)
  const setNetworkHost = vi.fn()
  const ref = new Proxy(
    { navigateToPath, navigateToParent, setNetworkHost },
    {
      get(target, prop) {
        if (prop in target) return (target as Record<string | symbol, unknown>)[prop]
        return () => undefined
      },
    },
  ) as unknown as FilePaneAPI
  return { ref, navigateToPath, navigateToParent, setNetworkHost }
}

export interface Harness {
  deps: NavigateDeps
  mgr: (pane: 'left' | 'right') => TabManager
  tab: (pane: 'left' | 'right') => ReturnType<typeof getActiveTab>
  persistEvents: PersistEvent[]
  lastUsedRecords: LastUsedPathRecord[]
  paneState: Record<'left' | 'right', { paneRef?: ReturnType<typeof makePaneRefStub> }>
  determineNavigationPath: ReturnType<typeof vi.fn>
  setFocusedPane: ReturnType<typeof vi.fn>
  addToast: ReturnType<typeof vi.fn>
}

/** The store-backed read/write deps over the real per-pane tab managers (live references, no snapshots). */
function makeStoreDeps(
  managers: Record<'left' | 'right', TabManager>,
  paneState: Record<'left' | 'right', { paneRef?: ReturnType<typeof makePaneRefStub> }>,
  tab: (pane: 'left' | 'right') => ReturnType<typeof getActiveTab>,
): Pick<
  NavigateDeps,
  | 'getTabMgr'
  | 'getPaneVolumeId'
  | 'getPanePath'
  | 'getPaneHistory'
  | 'getPaneVolumePath'
  | 'getPaneVolumeName'
  | 'otherPane'
  | 'setPaneVolumeId'
  | 'setPanePath'
  | 'setPaneHistory'
  | 'getPaneRef'
> {
  return {
    getTabMgr: (pane) => managers[pane],
    getPaneVolumeId: (pane) => tab(pane).volumeId,
    getPanePath: (pane) => tab(pane).path,
    getPaneHistory: (pane) => tab(pane).history,
    getPaneVolumePath: (pane) => volumePathFor(tab(pane).volumeId),
    getPaneVolumeName: (pane) => volumeNameFor(tab(pane).volumeId),
    otherPane: (pane) => (pane === 'left' ? 'right' : 'left'),
    setPaneVolumeId: (pane, volumeId) => {
      tab(pane).volumeId = volumeId
    },
    setPanePath: (pane, path) => {
      tab(pane).path = path
    },
    setPaneHistory: (pane, history) => {
      tab(pane).history = history
    },
    getPaneRef: (pane) => paneState[pane].paneRef?.ref,
  }
}

export interface HarnessOpts {
  left?: { path: string; volumeId: string }
  right?: { path: string; volumeId: string }
  suppressRef?: ('left' | 'right')[]
}

/** Builds one pane's tab manager + (optionally suppressed) FilePane stub from the opts. */
function makePaneFixture(spec: { path: string; volumeId: string } | undefined, suppressed: boolean) {
  const mgr = createTabManager(createInitialTabState(spec?.path ?? '/Users/me', spec?.volumeId ?? 'root'))
  const state: { paneRef?: ReturnType<typeof makePaneRefStub> } = suppressed ? {} : { paneRef: makePaneRefStub() }
  return { mgr, state }
}

/** Where each volume lands when nothing is remembered, when that isn't its root. */
const LANDINGS = new Map<string, string>([['sftp-nas', 'sftp://ada@nas.local:22/srv/data/photos']])

/** Builds a fresh harness: real per-pane tab managers + spied side effects. */
export function makeHarness(opts?: HarnessOpts): Harness {
  const suppress = new Set(opts?.suppressRef ?? [])
  const left = makePaneFixture(opts?.left, suppress.has('left'))
  const right = makePaneFixture(opts?.right, suppress.has('right'))
  const managers: Record<'left' | 'right', TabManager> = { left: left.mgr, right: right.mgr }
  const paneState: Record<'left' | 'right', { paneRef?: ReturnType<typeof makePaneRefStub> }> = {
    left: left.state,
    right: right.state,
  }

  const persistEvents: PersistEvent[] = []
  const lastUsedRecords: LastUsedPathRecord[] = []

  const determineNavigationPath = vi
    .fn()
    .mockImplementation((args: DetermineNavigationPathArgs) => Promise.resolve(args.targetPath))
  const setFocusedPane = vi.fn()
  const addToast = vi.fn()

  const tab = (pane: 'left' | 'right') => getActiveTab(managers[pane])

  const deps: NavigateDeps = {
    ...makeStoreDeps(managers, paneState, tab),
    setFocusedPane,
    getVolumePathById: (volumeId) => VOLUMES.get(volumeId)?.path,
    getVolumeLandingById: (volumeId) => LANDINGS.get(volumeId),
    determineNavigationPath,
    persist: (event) => {
      persistEvents.push(event)
      if (event.kind === 'last-used-path') lastUsedRecords.push(event.record)
    },
    addToast,
    tokens: new Map(),
    correctionGen: { value: 0 },
    returnPoints: new Map(),
    // The two virtual volumes render without a listing, as their capability rows say.
    volumeHasListing: (volumeId) => volumeId !== 'network' && volumeId !== 'search-results',
  }

  return {
    deps,
    mgr: (pane) => managers[pane],
    tab,
    persistEvents,
    lastUsedRecords,
    paneState,
    determineNavigationPath,
    setFocusedPane,
    addToast,
  }
}

/** Volume mount path for a pane's current volume (`smb://` for network, `/` for unknown). */
function volumePathFor(volumeId: string): string {
  if (volumeId === 'network') return 'smb://'
  return VOLUMES.get(volumeId)?.path ?? '/'
}

/** Display name for a pane's current volume (`Network` for the virtual volume, else the live name). */
function volumeNameFor(volumeId: string): string | undefined {
  if (volumeId === 'network') return 'Network'
  return VOLUMES.get(volumeId)?.name
}

/** Flush queued microtasks (the cross-volume async IIFE + correction `.then`). */
export async function flush(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
}
