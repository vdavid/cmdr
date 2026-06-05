/**
 * `navigate(intent, deps)` — the transactional navigation seam (Phase 3).
 *
 * One entry point for every coordinator-level pane navigation, replacing the
 * four-function braid in `DualPaneExplorer` (`handlePathChange` /
 * `applyPathChange` / `handleVolumeChange` / `applyVolumePathCorrection`) plus
 * the history walk, the snapshot-pane exit, and the five edge flows
 * (cancel / MTP-fatal / retry / open-home / unmount). It sits ON TOP of the
 * FilePane listing primitives (`navigateToPath` / `navigateToParent`); listing
 * mechanics stay pane-owned (master § Target architecture 3 scoping note).
 *
 * Built + tested in isolation against fake deps. M3 swaps the DPE callers onto
 * it and deletes the old braid + the `volumeChangeGeneration` counter.
 *
 * ## What `navigate()` owns vs. what stays pane-owned
 *
 * `navigate()` decides INTENT (volume/path/history/snapshot/edge), commits pane
 * STATE through one `commit()` point, fires PERSISTENCE through one trigger, and
 * then drives the FilePane listing primitive. The FilePane keeps its own
 * `loadGeneration` + `listingId` gate (P3, a different layer): `navigate()`'s
 * token is the coordinator-level intent layer, the pane's gate is the listing
 * layer.
 *
 * ## The single commit point
 *
 * Today volumeId + path + history are written separately across each braid
 * branch (`setPaneVolumeId` / `setPanePath` / `setPaneHistory`). Here they go
 * through one `commit(pane, { volumeId?, path, historyEntry?, pushHistory })`
 * call that the pinned-tab fork, the in-place path arm, the volume switch, the
 * history walk, and every edge flow share. After M3 + M5 the ONLY caller of the
 * per-pane mutators is this `commit` (master § M3 grep-to-zero gate).
 *
 * ## Transaction token (Q3 — FE-side request map keyed by side)
 *
 * `navigate()` mints a monotonic `txToken` per call and stores it as the pane's
 * current transaction (`deps.tokens`, a `Map<'left'|'right', number>` the caller
 * owns so it survives across `navigate()` calls). The background
 * `determineNavigationPath` correction (folding `applyVolumePathCorrection`) and
 * the in-place `onPathChange` re-entry each capture their token and bail on
 * `token !== current[pane]`. This subsumes the THREE coordinator staleness
 * mechanisms — the `applyPathChange` volume-prefix forensics (L6, the FE twin of
 * the banned `error-string-match`), the `volumeChangeGeneration` counter, and the
 * background-correction guard — into one compare. The drop-foreign-listings
 * _policy_ is identical (L6); only the _mechanism_ changes.
 *
 * **Same-token self-re-entry (M3 contract, stated here so M2 doesn't violate it):**
 * `commitPathFromListing` does NOT mint a new token. A parent-nav
 * (`{ history: 'parent' }`) or a deleted-folder walk-up
 * (`navigateToFallback`) re-enters via `onPathChange` carrying the SAME token as
 * the transaction that started it — so it passes the token compare and commits,
 * rather than looking stale and dropping the path push (which would regress
 * Back-depth). Only a fresh `navigate()` call advances the token.
 *
 * ## `swapPanes` token invariant (L4)
 *
 * The side-keyed token map is safe across a pane swap ONLY because
 * `canSwapPanes()` refuses while either pane `isLoading()` — no live transaction
 * can exist at swap time, so `swapPanes` never has to migrate or invalidate a
 * token between sides, and stays zero-IPC (L4). The token model relies on that
 * `isLoading()` gate; don't relax it.
 *
 * ## Per-arm optimism (P4, corrected after M1)
 *
 * Optimism is PER ARM — the M1 regression tests pin the split:
 * - **Volume switch** (`{ volumeId, path }` with `volumeId` set) commits
 *   volumeId + path + history SYNCHRONOUSLY before any listing (truly optimistic).
 * - **In-place path nav** (`{ path }`, same volume) does NOT commit on call: it
 *   drives `FilePane.navigateToPath`, and the pane's `onPathChange` at
 *   `listing-complete` lands the commit via `commitPathFromListing`. `navigate()`
 *   must NOT "upgrade" it to an immediate commit (that would change when the
 *   path/breadcrumb updates relative to the listing — a PR3 violation).
 *
 * What P4 forbids in BOTH arms: a new synchronous validate-then-commit gate. The
 * only synchronous work before each arm's commit is the capability/refusal checks
 * that are already synchronous today (the `network` refusal, the MTP refusal).
 * The cross-volume snapshot resolution (`resolveVolume`) stays
 * async-then-volume-switch, exactly as today.
 *
 * ## `settled` resolve point, PER INTENT ARM (caller-observable timing, PR3)
 *
 * - **In-place path nav + volume switch on a real volume**: `settled` is the
 *   FilePane `navigateToPath` promise — resolves on `listing-complete`.
 * - **Cross-volume snapshot arm**: `settled` resolves when the volume-switch
 *   commit is DONE, BEFORE the new listing loads (today's async IIFE resolves
 *   when fire-and-forget `handleVolumeChange` returns). `navigate-and-select` /
 *   `handleSearchNavigate` bridge the gap themselves via `moveCursor`'s internal
 *   `whenLoadSettles` (L2-adjacent); don't "fix" this to await the listing.
 * - **Network / no-volume-resolved / state-restore-only branches**: `settled`
 *   resolves immediately (a resolved no-op), matching today's no-load branches.
 * - **History / edge flows**: match whichever primitive they drive.
 */

import type { FilePaneAPI } from './types'
import type { TabManager } from '../tabs/tab-state-manager.svelte'
import { getActiveTab, pushHistoryEntry, MAX_TABS_PER_PANE } from '../tabs/tab-state-manager.svelte'
import type { TabState } from '../tabs/tab-types'
import {
  pushPath,
  back,
  forward,
  getCurrentEntry,
  canGoBack,
  canGoForward,
  createHistory,
  type NavigationHistory,
  type HistoryEntry,
} from '../navigation/navigation-history'
import { isPathOnVolume } from '../navigation/path-navigation'
import { isCrossVolumeNavigation } from './snapshot-pane-navigation'

/** Where a navigation originates. Drives focus + history-push behavior, never the destination. */
export type NavigateSource = 'user' | 'mcp' | 'history' | 'correction' | 'cancel' | 'fallback' | 'mirror'

/** The destination of a navigation: a volume/path change, a history walk, or a snapshot open. */
export type NavigateTo =
  | { volumeId?: string; path: string } // volume change (volumeId set) OR in-place path nav (volumeId omitted ⇒ same volume)
  | { history: 'back' | 'forward' | 'parent' }
  | { snapshot: string } // search-results snapshot id; routes through the volume-change machinery

export interface NavigateIntent {
  pane: 'left' | 'right'
  to: NavigateTo
  source: NavigateSource
  /** Land the cursor on this entry after the listing settles (the FilePane selectName channel). */
  selectName?: string
}

/** Why a synchronous navigation refused. `message` is the exact current string — contract (L12). */
export interface NavigateRefusal {
  kind: 'on-network-volume' | 'mtp-unconnected' | 'pane-unavailable' | 'no-volume-resolved'
  /** EXACT current refusal string, forwarded verbatim as the `mcp-response` error. Pinned byte-for-byte. */
  message: string
}

/**
 * The typed replacement for today's `navigateToPath` `string | Promise<void>`.
 * `started.settled` resolves when the listing completes (or per the per-arm
 * contract above); `refused` replaces the sync `string` sentinel three external
 * callers branch on via `typeof result === 'string'`.
 */
export type NavigateResult =
  | { status: 'started'; settled: Promise<void> }
  | { status: 'refused'; reason: NavigateRefusal }

/** A single state commit: volumeId (optional ⇒ unchanged) + path + an optional history entry to push. */
export interface NavigateCommit {
  pane: 'left' | 'right'
  /** When set, the pane switches volume. Omitted ⇒ same-volume path commit. */
  volumeId?: string
  path: string
  /**
   * History push policy. `'push-path'` pushes a same-volume path entry (the
   * in-place arm). `'push-entry'` pushes `{ volumeId, path, networkHost? }` (the
   * volume-switch + edge-flow arms). `'none'` commits state without touching
   * history (the volume-unmount redirect — its no-history-push asymmetry, M5).
   */
  history: 'push-path' | 'push-entry' | 'none'
  /** For `'push-entry'` on the network volume: the host to carry on the entry. */
  networkHost?: HistoryEntry['networkHost']
}

/** Last-used-path record: `{volumeId, path}`. Fired through the persistence trigger. */
export interface LastUsedPathRecord {
  volumeId: string
  path: string
}

/**
 * Everything `navigate()` reads or writes, injected so the transaction is
 * headless-testable against fakes. Mirrors the Phase-0 factory pattern
 * (`createPaneCommands(access, dialogs)`): the app builds these from
 * `DualPaneExplorer`'s store + FilePane handles; tests pass fakes.
 */
export interface NavigateDeps {
  // --- store reads (live references, never snapshots) ---
  getTabMgr: (pane: 'left' | 'right') => TabManager
  getPaneVolumeId: (pane: 'left' | 'right') => string
  getPanePath: (pane: 'left' | 'right') => string
  getPaneHistory: (pane: 'left' | 'right') => NavigationHistory
  /** The pane's volume mount path (`smb://` for network), used by the stale-listing drop policy. */
  getPaneVolumePath: (pane: 'left' | 'right') => string
  /** The pane's volume display name, used by the on-network / on-MTP refusal strings. */
  getPaneVolumeName: (pane: 'left' | 'right') => string | undefined
  otherPane: (pane: 'left' | 'right') => 'left' | 'right'

  // --- store writes (the ONLY callers of these after M3/M5 are inside this module) ---
  setPaneVolumeId: (pane: 'left' | 'right', volumeId: string) => void
  setPanePath: (pane: 'left' | 'right', path: string) => void
  setPaneHistory: (pane: 'left' | 'right', history: NavigationHistory) => void
  setFocusedPane: (pane: 'left' | 'right') => void

  // --- FilePane handle ---
  getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined

  // --- volume resolution + defaults ---
  /** `resolvePathVolume` — resolves a real path to its containing volume. Injectable for tests. */
  resolveVolume: (path: string) => Promise<{ volume: { id: string; path: string } | null }>
  getDefaultVolumeId: () => Promise<string>
  /** The volume's mount path by id, or undefined when not in the live list. */
  getVolumePathById: (volumeId: string) => string | undefined
  /** Background "best path" resolver (`determineNavigationPath`), gated by the token. */
  determineNavigationPath: (
    volumeId: string,
    volumePath: string,
    targetPath: string,
    otherPane: { otherPaneVolumeId: string; otherPanePath: string },
  ) => Promise<string>
  /** Walk-up to the nearest existing dir (`resolveValidPath`), for the cancel branch. */
  resolveValidPath: (path: string, options: { volumeRoot?: string }) => Promise<string | null>
  /** `pathExists`, for the unmount-redirect home/`/` fallback. */
  pathExists: (path: string) => Promise<boolean>

  // --- side effects ---
  /** Persistence trigger. In M2 a no-op spy; M4 wires the single subscriber. */
  persist: (event: PersistEvent) => void
  /** Re-anchor DOM focus on the explorer container (cancel / fallback flows focus; volume-select does NOT). */
  focusContainer: () => void
  /** Refresh the volume selector after a successful unreachable retry. */
  requestVolumeRefresh: () => void
  /** Warn toast (the `MAX_TABS_PER_PANE` "Tab limit reached" branch). */
  addToast: (message: string, opts: { level: 'warn' }) => void

  // --- the per-pane transaction token map (caller-owned so it survives across calls) ---
  tokens: Map<'left' | 'right', number>
}

/**
 * Persistence events emitted by `navigate()`. The M2 trigger is a no-op spy that
 * tests assert on; M4 turns it into the single debounced+diffed subscriber that
 * absorbs the scattered `saveAppStatus` / `saveTabsForPaneSide` /
 * `saveLastUsedPathForVolume` sites. Until then `navigate()` emits the same
 * INTENT each old call site had, so M4 can fan them out without re-deriving them.
 */
export type PersistEvent =
  | { kind: 'pane-state'; pane: 'left' | 'right' }
  | { kind: 'last-used-path'; record: LastUsedPathRecord }

/** Exact refusal strings — contract (L12). Pinned byte-for-byte by the M2 + M1 suites. */
function onNetworkRefusal(volumeLabel: string): NavigateRefusal {
  return {
    kind: 'on-network-volume',
    message: `Pane is on the ${volumeLabel} volume. Use select_volume to switch to a local volume first.`,
  }
}

const PANE_UNAVAILABLE_REFUSAL: NavigateRefusal = { kind: 'pane-unavailable', message: 'Pane not available' }

/**
 * MTP capability check (the `paneCommands.validateMtpNavigation` logic, inlined
 * so `navigate()` carries the exact strings). Returns a refusal or `null`.
 * Note the em dash in the first string — it's contract.
 */
function validateMtpNavigation(path: string, volumeId: string, volumeName: string | undefined): NavigateRefusal | null {
  if (path.startsWith('mtp://')) {
    const mtpMatch = path.match(/^mtp:\/\/([^/]+)\/(\d+)/)
    const pathDeviceId = mtpMatch?.[1]
    const pathStorageId = mtpMatch?.[2]
    if (!pathDeviceId || !pathStorageId || volumeId !== `${pathDeviceId}:${pathStorageId}`) {
      return { kind: 'mtp-unconnected', message: `Pane is not on this MTP volume — call select_volume first.` }
    }
  } else if (volumeId.includes(':') && volumeId.startsWith('mtp-')) {
    return {
      kind: 'mtp-unconnected',
      message: `Pane is on the ${volumeName ?? volumeId} MTP volume. Use select_volume to switch to a local volume first.`,
    }
  }
  return null
}

/** A resolved no-op `settled` — for branches that commit state without driving a listing. */
const SETTLED_NOOP: Promise<void> = Promise.resolve()

/**
 * Mints + stores a fresh transaction token for `pane`, returning it. Every fresh
 * `navigate()` call advances the pane's token; the self-re-entry path
 * (`commitPathFromListing`) deliberately does NOT call this.
 */
function mintToken(deps: NavigateDeps, pane: 'left' | 'right'): number {
  const next = (deps.tokens.get(pane) ?? 0) + 1
  deps.tokens.set(pane, next)
  return next
}

/** True while `token` is still the pane's active transaction (not superseded by a newer `navigate()`). */
function tokenLive(deps: NavigateDeps, pane: 'left' | 'right', token: number): boolean {
  return deps.tokens.get(pane) === token
}

/**
 * The single state-commit point. Writes volumeId (when switching) + path + an
 * optional history entry together, then fires the pane-state persistence intent.
 * After M3/M5 this is the ONLY caller of `setPaneVolumeId` / `setPanePath` /
 * `setPaneHistory` outside the per-pane mutators themselves.
 */
function commit(deps: NavigateDeps, c: NavigateCommit): void {
  if (c.volumeId !== undefined) deps.setPaneVolumeId(c.pane, c.volumeId)
  deps.setPanePath(c.pane, c.path)

  if (c.history === 'push-path') {
    deps.setPaneHistory(c.pane, pushPath(deps.getPaneHistory(c.pane), c.path))
  } else if (c.history === 'push-entry') {
    const volumeId = c.volumeId ?? deps.getPaneVolumeId(c.pane)
    const entry: HistoryEntry = { volumeId, path: c.path }
    if (c.networkHost !== undefined) entry.networkHost = c.networkHost
    deps.setPaneHistory(c.pane, pushHistoryEntry(deps.getPaneHistory(c.pane), entry))
  }

  deps.persist({ kind: 'pane-state', pane: c.pane })
}

/**
 * The pinned-tab fork (L7 — unified HERE, the only place it's allowed to unify).
 * When the active tab is pinned and the destination differs, open a NEW unpinned
 * tab carrying the target instead of navigating in-place. At `MAX_TABS_PER_PANE`,
 * toast "Tab limit reached" and fall through to in-place. Returns `true` when a
 * new tab was opened (caller skips the in-place commit), `false` otherwise.
 *
 * Parameterized by `{ volumeId?, path }`: `volumeId` set ⇒ the volume-switch fork
 * (DPE:618), omitted ⇒ the in-place path fork (DPE:413). The two near-identical
 * branches become one.
 */
function tryPinnedFork(
  deps: NavigateDeps,
  pane: 'left' | 'right',
  target: { volumeId?: string; path: string },
): boolean {
  const mgr = deps.getTabMgr(pane)
  const activeTab = getActiveTab(mgr)

  const destinationDiffers =
    target.volumeId !== undefined
      ? target.volumeId !== activeTab.volumeId || target.path !== activeTab.path
      : target.path !== activeTab.path

  if (!activeTab.pinned || !destinationDiffers) return false

  if (mgr.tabs.length >= MAX_TABS_PER_PANE) {
    deps.addToast('Tab limit reached', { level: 'warn' })
    return false // fall through to in-place
  }

  const volumeId = target.volumeId ?? activeTab.volumeId
  const newTab: TabState = {
    id: crypto.randomUUID(),
    path: target.path,
    volumeId,
    history: createHistory(volumeId, target.path),
    sortBy: activeTab.sortBy,
    sortOrder: activeTab.sortOrder,
    viewMode: activeTab.viewMode,
    pinned: false,
    cursorFilename: null,
    unreachable: null,
  }

  const activeIndex = mgr.tabs.findIndex((t) => t.id === activeTab.id)
  mgr.tabs.splice(activeIndex + 1, 0, newTab)
  mgr.activeTabId = newTab.id
  deps.persist({ kind: 'pane-state', pane })
  return true
}

/**
 * The background "best path" correction (folds `applyVolumePathCorrection`,
 * DPE:669), gated by the transaction token instead of `volumeChangeGeneration`.
 * A correction whose token was superseded by a newer `navigate()` is dropped.
 */
function scheduleVolumePathCorrection(
  deps: NavigateDeps,
  pane: 'left' | 'right',
  token: number,
  volumeId: string,
  volumePath: string,
  targetPath: string,
): void {
  const other = deps.otherPane(pane)
  void deps
    .determineNavigationPath(volumeId, volumePath, targetPath, {
      otherPaneVolumeId: deps.getPaneVolumeId(other),
      otherPanePath: deps.getPanePath(other),
    })
    .then((betterPath) => {
      if (!tokenLive(deps, pane, token)) return
      if (betterPath !== targetPath && betterPath !== deps.getPanePath(pane)) {
        deps.setPanePath(pane, betterPath)
        deps.setPaneHistory(pane, pushHistoryEntry(deps.getPaneHistory(pane), { volumeId, path: betterPath }))
        deps.persist({ kind: 'pane-state', pane })
      }
    })
}

/**
 * A volume switch: the pinned fork, the single commit (volumeId + path + history),
 * focus shift, and the background correction. The shared body behind the
 * `{ volumeId, path }` arm, the snapshot arm, `selectVolumeBy*`, and the mirror
 * helpers. Commits SYNCHRONOUSLY (P4 — optimistic), then schedules the correction.
 */
function commitVolumeSwitch(
  deps: NavigateDeps,
  pane: 'left' | 'right',
  token: number,
  volumeId: string,
  volumePath: string,
  targetPath: string,
  options: { shiftFocus: boolean; networkHost?: HistoryEntry['networkHost'] },
): void {
  const activeTab = getActiveTab(deps.getTabMgr(pane))
  const oldPath = activeTab.path
  // Record the OLD path as the last-used for the OLD volume before the swap.
  deps.persist({ kind: 'last-used-path', record: { volumeId: activeTab.volumeId, path: oldPath } })

  if (tryPinnedFork(deps, pane, { volumeId, path: targetPath })) {
    if (options.shiftFocus) deps.setFocusedPane(pane)
    deps.persist({ kind: 'pane-state', pane })
    scheduleVolumePathCorrection(deps, pane, token, volumeId, volumePath, targetPath)
    return
  }

  commit(deps, {
    pane,
    volumeId,
    path: targetPath,
    history: 'push-entry',
    networkHost: options.networkHost,
  })
  if (options.shiftFocus) deps.setFocusedPane(pane)
  scheduleVolumePathCorrection(deps, pane, token, volumeId, volumePath, targetPath)
}

/**
 * The single entry point for every coordinator-level pane navigation. See the
 * module doc for the per-arm optimism + `settled` contract and the token model.
 */
export function navigate(intent: NavigateIntent, deps: NavigateDeps): NavigateResult {
  const { pane, to } = intent

  if ('history' in to) {
    return navigateHistory(deps, pane, to.history)
  }

  if ('snapshot' in to) {
    return navigateSnapshot(deps, pane, to.snapshot)
  }

  return navigateToVolumeOrPath(deps, intent, to)
}

/** The `{ volumeId?, path }` arm: volume switch (volumeId set) or in-place path nav (omitted). */
function navigateToVolumeOrPath(
  deps: NavigateDeps,
  intent: NavigateIntent,
  to: { volumeId?: string; path: string },
): NavigateResult {
  const { pane, source } = intent
  const currentVolumeId = deps.getPaneVolumeId(pane)
  const currentVolumeName = deps.getPaneVolumeName(pane)

  // --- Volume switch (volumeId present): truly optimistic, synchronous commit. ---
  if (to.volumeId !== undefined) {
    const token = mintToken(deps, pane)
    const volumePath = deps.getVolumePathById(to.volumeId) ?? to.path
    commitVolumeSwitch(deps, pane, token, to.volumeId, volumePath, to.path, {
      // Mirror/correction sources don't shift focus (L1: restoreFocus semantics).
      shiftFocus: source !== 'mirror' && source !== 'correction',
    })
    return { status: 'started', settled: SETTLED_NOOP }
  }

  // --- In-place path nav (same volume). ---

  // On-network-volume refusal (synchronous, today's DPE:1560).
  if (currentVolumeId === 'network') {
    return { status: 'refused', reason: onNetworkRefusal(currentVolumeName ?? currentVolumeId) }
  }

  // Snapshot-pane exit: a real path while on the search-results volume routes
  // through the volume-change machinery (L5 — must match FilePane.handleNavigate).
  if (isCrossVolumeNavigation(currentVolumeId, to.path)) {
    const token = mintToken(deps, pane)
    const settled = (async () => {
      const result = await deps.resolveVolume(to.path)
      const volume = result.volume
      if (!volume) {
        // no-volume-resolved: today this logs + returns undefined (a no-op), NOT
        // a refusal. The MCP round-trip sees ok:true with no nav. Preserve that.
        return
      }
      if (!tokenLive(deps, pane, token)) return
      commitVolumeSwitch(deps, pane, token, volume.id, volume.path, to.path, { shiftFocus: source !== 'mirror' })
    })()
    return { status: 'started', settled }
  }

  // MTP capability refusal (synchronous).
  const mtpRefusal = validateMtpNavigation(to.path, currentVolumeId, currentVolumeName)
  if (mtpRefusal) return { status: 'refused', reason: mtpRefusal }

  // Pinned-tab fork: open a new tab instead of in-place (L7).
  if (tryPinnedFork(deps, pane, { path: to.path })) {
    // The new tab is committed; the FilePane it mounts loads its own listing.
    // Matches today's handlePathChange pinned branch (no paneRef.navigateToPath).
    return { status: 'started', settled: SETTLED_NOOP }
  }

  // In-place: drive the FilePane primitive. The commit lands LATER, when the
  // listing completes and the pane's onPathChange fires `commitPathFromListing`
  // (P4 — in-place arm is NOT optimistic). `settled` is the FilePane promise.
  const paneRef = deps.getPaneRef(pane)
  if (!paneRef) return { status: 'refused', reason: PANE_UNAVAILABLE_REFUSAL }
  return { status: 'started', settled: paneRef.navigateToPath(to.path, intent.selectName) }
}

/**
 * The `{ snapshot }` arm: build `search-results://<id>` and route through the
 * volume-change machinery so `pushHistoryEntry` increments the snapshot refcount
 * (the snapshot-store integration). Mirrors `openSearchSnapshotInPane` →
 * `handleVolumeChange('search-results', url, url)`.
 */
function navigateSnapshot(deps: NavigateDeps, pane: 'left' | 'right', snapshotId: string): NavigateResult {
  const url = `search-results://${snapshotId}`
  const token = mintToken(deps, pane)
  commitVolumeSwitch(deps, pane, token, 'search-results', url, url, { shiftFocus: true })
  return { status: 'started', settled: SETTLED_NOOP }
}

/** The `{ history }` arm: back / forward walk the stack; parent delegates to the FilePane primitive. */
function navigateHistory(
  deps: NavigateDeps,
  pane: 'left' | 'right',
  action: 'back' | 'forward' | 'parent',
): NavigateResult {
  const paneRef = deps.getPaneRef(pane)

  if (action === 'parent') {
    // Delegates to the FilePane primitive; its onPathChange re-enters as a
    // same-token self-re-entry (commitPathFromListing). `settled` is the primitive's.
    if (!paneRef) return { status: 'started', settled: SETTLED_NOOP }
    return { status: 'started', settled: paneRef.navigateToParent().then(() => undefined) }
  }

  const history = deps.getPaneHistory(pane)
  let newHistory: NavigationHistory
  if (action === 'back' && canGoBack(history)) {
    newHistory = back(history)
  } else if (action === 'forward' && canGoForward(history)) {
    newHistory = forward(history)
  } else {
    return { status: 'started', settled: SETTLED_NOOP }
  }

  const target = getCurrentEntry(newHistory)
  commitHistoryWalk(deps, pane, newHistory, target.path)
  return { status: 'started', settled: SETTLED_NOOP }
}

/**
 * Commits a history-walk destination (folds `updatePaneAfterHistoryNavigation`,
 * DPE:1023): set history + path, switch volume if the target entry crosses
 * volumes, record last-used-path, and restore the network host on a network entry.
 */
function commitHistoryWalk(
  deps: NavigateDeps,
  pane: 'left' | 'right',
  newHistory: NavigationHistory,
  targetPath: string,
): void {
  const entry = getCurrentEntry(newHistory)
  const paneRef = deps.getPaneRef(pane)

  deps.setPaneHistory(pane, newHistory)
  deps.setPanePath(pane, targetPath)
  if (entry.volumeId !== deps.getPaneVolumeId(pane)) {
    deps.setPaneVolumeId(pane, entry.volumeId)
  }
  deps.persist({ kind: 'pane-state', pane })
  deps.persist({ kind: 'last-used-path', record: { volumeId: entry.volumeId, path: targetPath } })

  if (entry.volumeId === 'network') {
    paneRef?.setNetworkHost(entry.networkHost ?? null)
  }
}

/**
 * The in-place path commit landed from a FilePane `onPathChange` (folds
 * `applyPathChange`, DPE:447). Drops stale listings by the token + the
 * drop-foreign-listings policy (L6 — policy identical, mechanism is the token).
 *
 * **Token semantics:** this is NOT a fresh `navigate()`; it does NOT mint a token.
 * It runs at `listing-complete` for the transaction the in-place / parent-nav /
 * walk-up arm started. Because no newer `navigate()` has minted a token in the
 * meantime (the user hasn't navigated away), the pane's current token still
 * matches the one the transaction captured — a self-re-entry passes, a genuinely
 * stale listing (the user flipped volumes ⇒ a newer token) is dropped by the
 * foreign-path policy below.
 *
 * Returns whether the path was committed (for tests + M3 wiring).
 */
export function commitPathFromListing(deps: NavigateDeps, pane: 'left' | 'right', path: string): boolean {
  const currentVolumeId = deps.getPaneVolumeId(pane)
  const currentVolumePath = deps.getPaneVolumePath(pane)

  // Drop-foreign-listings policy (L6, identical to applyPathChange): a virtual
  // volume checks its URL prefix; a real volume uses isPathOnVolume.
  if (currentVolumeId === 'network' || currentVolumeId === 'search-results') {
    const expectedPrefix = currentVolumeId === 'network' ? 'smb://' : 'search-results://'
    if (!path.startsWith(expectedPrefix)) return false
  } else if (!isPathOnVolume(path, currentVolumePath)) {
    return false
  }

  commit(deps, { pane, path, history: 'push-path' })
  deps.persist({ kind: 'last-used-path', record: { volumeId: deps.getPaneVolumeId(pane), path } })
  return true
}
