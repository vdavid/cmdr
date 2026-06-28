/**
 * `navigate(intent, deps)` — the transactional navigation seam.
 *
 * The single entry point for every coordinator-level pane navigation: the
 * volume switch, the in-place path nav, the history walk, the snapshot-pane
 * exit, and the five edge flows (cancel / MTP-fatal / retry / open-home /
 * unmount). It sits ON TOP of the FilePane listing primitives
 * (`navigateToPath` / `navigateToParent`); listing mechanics stay pane-owned
 * (master § Target architecture 3 scoping note). `DualPaneExplorer` builds the
 * `NavigateDeps` from its store + FilePane handles and wraps this as its
 * `navigate` export; tests pass fakes.
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
 * volumeId + path + history go through one `commit(pane, { volumeId?, path,
 * history, networkHost? })` call that the pinned-tab fork, the in-place path
 * arm, the volume switch, the history walk, and every edge flow share. The ONLY
 * caller of the per-pane mutators (`setPaneVolumeId` / `setPanePath` /
 * `setPaneHistory`) is this `commit` — the sole exceptions are the two
 * orthogonal network-host pushes (`handleNetworkHostChange`,
 * `mirrorNetworkStateToPane`) that carry an SMB host onto the history entry.
 *
 * ## Transaction token (Q3 — FE-side request map keyed by side)
 *
 * `navigate()` mints a monotonic `txToken` per call and stores it as the pane's
 * current transaction (`deps.tokens`, a `Map<'left'|'right', number>` the caller
 * owns so it survives across `navigate()` calls). The token gates the per-pane
 * cross-volume resolve bail (a slow `resolveVolume` whose pane got re-navigated is
 * dropped). The in-place `onPathChange` re-entry (`commitPathFromListing`) drops a
 * stale listing by the foreign-path policy (L6, the FE twin of the banned
 * `error-string-match`) — NOT by the token. The background
 * `determineNavigationPath` correction (folding `applyVolumePathCorrection`) is
 * gated by `deps.correctionGen`, a SINGLE GLOBAL counter shared by both panes
 * (exactly the old `volumeChangeGeneration`): a later volume change on EITHER pane
 * supersedes a pending correction. Per-pane gating there would let both panes'
 * corrections run after a simultaneous two-pane reset and re-enter the
 * listing/onPathChange cycle on both panes — a webview freeze. Together these
 * replace the three coordinator staleness mechanisms; the drop-foreign-listings
 * _policy_ is identical (L6), only the _mechanism_ changes.
 *
 * **Same-token self-re-entry:**
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
 * ## Per-arm optimism (P4)
 *
 * Optimism is PER ARM — the navigation-transaction regression tests pin the split:
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
import { tString } from '$lib/intl/messages.svelte'
import type { Location } from '$lib/tauri-commands'

/** Where a navigation originates. Drives focus + history-push behavior, never the destination. */
export type NavigateSource = 'user' | 'mcp' | 'history' | 'correction' | 'cancel' | 'fallback' | 'mirror'

/**
 * The destination of a navigation: a `Location` (go somewhere), a deliberate
 * volume (re)select, a history walk, or a snapshot open. `Location` is
 * navigation's currency — a `(volumeId, path)` pair resolved at the four edges
 * (⌘G, MCP `nav_to_path`, search-result activation, downloads reveal) via
 * `navigation/resolve-location.ts`. `{ location }` routes itself: same volume →
 * in-place arm, different volume → switch arm. `{ volumeId, path }` is the
 * deliberate volume-(re)select intent that ALWAYS takes the switch arm (its
 * callers legitimately pass the CURRENT volume id to re-select it).
 */
export type NavigateTo =
  | { location: Location } // go to a location; volumeId mandatory, routes to the in-place or switch arm
  | { volumeId?: string; path: string } // volume change (volumeId set) OR in-place path nav (volumeId omitted ⇒ same volume)
  | { history: 'back' | 'forward' | 'parent' }
  | { snapshot: string } // search-results snapshot id; routes through the volume-change machinery

export interface NavigateIntent {
  pane: 'left' | 'right'
  to: NavigateTo
  source: NavigateSource
  /** Land the cursor on this entry after the listing settles (the FilePane selectName channel). */
  selectName?: string
  /**
   * Whether a `{ volumeId, path }` volume switch pushes a history entry. Defaults
   * to `true`. The volume-unmount redirect sets it `false`: ejecting a volume
   * redirects each affected pane to the default volume at `~` WITHOUT growing a
   * Back target (the history-push asymmetry — the MTP-fatal / retry / open-home
   * fallbacks DO push, the unmount redirect does NOT). Encoded as an intent field
   * rather than a distinct source because the four `'fallback'` flows share their
   * focus behavior (none shift the focused pane) and differ only in this push.
   */
  pushHistory?: boolean
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
   * history (the volume-unmount redirect — its no-history-push asymmetry).
   */
  history: 'push-path' | 'push-entry' | 'none'
  /** For `'push-entry'` on the network volume: the host to carry on the entry. */
  networkHost?: HistoryEntry['networkHost']
}

/** Last-used-path record: a `Location` (volumeId + path). Fired through the persistence trigger. */
export type LastUsedPathRecord = Location

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

  // --- store writes (the only callers of these are this module's `commit`) ---
  setPaneVolumeId: (pane: 'left' | 'right', volumeId: string) => void
  setPanePath: (pane: 'left' | 'right', path: string) => void
  setPaneHistory: (pane: 'left' | 'right', history: NavigationHistory) => void
  setFocusedPane: (pane: 'left' | 'right') => void

  // --- FilePane handle ---
  getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined

  // --- volume resolution + defaults ---
  /** `resolvePathVolume` — resolves a real path to its containing volume. Injectable for tests. */
  resolveVolume: (path: string) => Promise<{ volume: { id: string; path: string } | null }>
  /** The volume's mount path by id, or undefined when not in the live list. */
  getVolumePathById: (volumeId: string) => string | undefined
  /** Background "best path" resolver (`determineNavigationPath`), gated by the token. */
  determineNavigationPath: (
    volumeId: string,
    volumePath: string,
    targetPath: string,
    otherPane: { otherPaneVolumeId: string; otherPanePath: string },
  ) => Promise<string>

  // --- side effects ---
  /** Persistence trigger fed to the single nav-state persistence subscriber (A5). */
  persist: (event: PersistEvent) => void
  /** Warn toast (the `MAX_TABS_PER_PANE` "Tab limit reached" branch). */
  addToast: (message: string, opts: { level: 'warn' }) => void

  // --- the per-pane transaction token map (caller-owned so it survives across calls) ---
  tokens: Map<'left' | 'right', number>
  /**
   * The GLOBAL background-correction generation (the old `volumeChangeGeneration`
   * counter — a SINGLE counter shared by both panes, NOT per-pane). A mutable
   * holder so it survives across `navigate()` calls. Each scheduled correction
   * bumps `.value` and captures it; a later volume change on EITHER pane bumps it
   * again, dropping the stale correction. Caller-owned, like `tokens`.
   */
  correctionGen: { value: number }
}

/**
 * Persistence events emitted by `navigate()`, consumed by the single nav-state
 * persistence subscriber (A5). `pane-state` is covered REACTIVELY there (the
 * subscriber's per-pane effects watch the store mutation `commit` makes), so the
 * trigger is a no-op for it; `last-used-path` is a DELTA (the old path of the old
 * volume on a switch) the subscriber can't derive from a snapshot, so it's
 * forwarded explicitly.
 */
export type PersistEvent =
  | { kind: 'pane-state'; pane: 'left' | 'right' }
  | { kind: 'last-used-path'; record: LastUsedPathRecord }

/** Exact refusal strings — contract (L12). Pinned byte-for-byte by the navigate suites. */
function onNetworkRefusal(volumeLabel: string): NavigateRefusal {
  return {
    kind: 'on-network-volume',
    message: `Pane is on the ${volumeLabel} volume. Use select_volume to switch to a local volume first.`,
  }
}

const PANE_UNAVAILABLE_REFUSAL: NavigateRefusal = { kind: 'pane-unavailable', message: 'Pane not available' }

/**
 * MTP capability check. Returns a refusal or `null`. Note the em dash in the
 * first string — it's contract (L12), byte-pinned by `navigate.test.ts`.
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
 * Whether a volume switch shifts the focused pane (L1). A direct user/MCP/history
 * navigation makes the navigated pane the focused one; `'mirror'` (copy-path
 * between panes — restoreFocus semantics) and `'correction'` (a background
 * refinement) keep focus put. The edge-flow recoveries (`'cancel'` walk-up /
 * network-restore, `'fallback'` MTP-fatal / retry / open-home / unmount) also keep
 * the focused pane put — they re-anchor DOM focus on the container instead, via
 * the handler's own `focusContainer()` where today's code does.
 */
function shiftsFocus(source: NavigateSource): boolean {
  return source === 'user' || source === 'mcp' || source === 'history'
}

/**
 * The single state-commit point. Writes volumeId (when switching) + path + an
 * optional history entry together, then fires the pane-state persistence intent.
 * This is the ONLY caller of `setPaneVolumeId` / `setPanePath` / `setPaneHistory`
 * outside the per-pane mutators themselves (the two orthogonal network-host
 * pushes aside).
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
 * Splice a NEW unpinned tab (carrying `{ volumeId, path }`) directly after the
 * pinned active tab and make it active. Inherits the active tab's sort + view.
 * Shared tab-creation for the two pinned forks (volume-switch + in-place path);
 * each fork owns its own persistence afterward (the two differ in their
 * last-used-path semantics, so persistence is NOT shared here — see callers).
 */
function spliceNewUnpinnedTab(mgr: TabManager, activeTab: TabState, target: { volumeId: string; path: string }): void {
  const newTab: TabState = {
    id: crypto.randomUUID(),
    path: target.path,
    volumeId: target.volumeId,
    history: createHistory(target.volumeId, target.path),
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
}

/**
 * The VOLUME-SWITCH half of the pinned-tab fork (L7, folds `handleVolumeChange`'s
 * pinned branch, DPE:618). When the active tab is pinned and the destination
 * volume/path differs, open a NEW unpinned tab; at `MAX_TABS_PER_PANE`, toast and
 * fall through to in-place. Returns `true` when a new tab was opened (caller skips
 * the in-place commit). The OLD path's last-used-path pre-save happens in
 * `commitVolumeSwitch` (the OLD path), so the fork itself only persists tab state.
 */
function tryPinnedVolumeFork(
  deps: NavigateDeps,
  pane: 'left' | 'right',
  target: { volumeId: string; path: string },
): boolean {
  const mgr = deps.getTabMgr(pane)
  const activeTab = getActiveTab(mgr)

  if (!activeTab.pinned || (target.volumeId === activeTab.volumeId && target.path === activeTab.path)) return false

  if (mgr.tabs.length >= MAX_TABS_PER_PANE) {
    deps.addToast(tString('fileExplorer.tabs.limitReached'), { level: 'warn' })
    return false // fall through to in-place
  }

  spliceNewUnpinnedTab(mgr, activeTab, target)
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
  const correctionGen = (deps.correctionGen.value += 1)
  const other = deps.otherPane(pane)
  void deps
    .determineNavigationPath(volumeId, volumePath, targetPath, {
      otherPaneVolumeId: deps.getPaneVolumeId(other),
      otherPanePath: deps.getPanePath(other),
    })
    .then((betterPath) => {
      // GLOBAL supersede (matches the old `volumeChangeGeneration`, which was a
      // single counter shared by BOTH panes): a later volume change on EITHER
      // pane drops this pending correction. A per-pane token would let both
      // panes' corrections run after a simultaneous two-pane reset (the E2E
      // `ensureAppReady` double `mcp-volume-select`), which re-enters the
      // listing/onPathChange cycle on both panes at once and freezes the
      // webview. The cross-volume resolve bail stays per-pane (`token`); only
      // the correction supersede is global.
      if (correctionGen !== deps.correctionGen.value) return
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
 * `{ volumeId, path }` arm, the snapshot arm, `selectVolumeBy*`, the mirror
 * helpers, and the edge-flow fallbacks. Commits SYNCHRONOUSLY (P4 — optimistic),
 * then schedules the correction.
 *
 * `options.terminal` marks an edge-flow fallback (MTP-fatal / retry / open-home /
 * unmount): the destination is a fixed recovery target the user/error already
 * resolved, so there's no OLD-path pre-save (the old volume is broken/gone) and
 * no background `determineNavigationPath` correction (no "best path" to refine —
 * the recovery target IS the answer). Matches today's bespoke handlers, which
 * neither save the old path nor run a correction. `options.pushHistory: false`
 * additionally commits with `history: 'none'` (the unmount redirect's
 * no-Back-target asymmetry); the other three fallbacks push an entry.
 */
function commitVolumeSwitch(
  deps: NavigateDeps,
  pane: 'left' | 'right',
  token: number,
  volumeId: string,
  volumePath: string,
  targetPath: string,
  options: {
    shiftFocus: boolean
    networkHost?: HistoryEntry['networkHost']
    terminal?: boolean
    pushHistory?: boolean
  },
): void {
  if (!options.terminal) {
    const activeTab = getActiveTab(deps.getTabMgr(pane))
    // Record the OLD path as the last-used for the OLD volume before the swap.
    deps.persist({ kind: 'last-used-path', record: { volumeId: activeTab.volumeId, path: activeTab.path } })
  }

  if (!options.terminal && tryPinnedVolumeFork(deps, pane, { volumeId, path: targetPath })) {
    if (options.shiftFocus) deps.setFocusedPane(pane)
    deps.persist({ kind: 'pane-state', pane })
    scheduleVolumePathCorrection(deps, pane, token, volumeId, volumePath, targetPath)
    return
  }

  commit(deps, {
    pane,
    volumeId,
    path: targetPath,
    history: options.pushHistory === false ? 'none' : 'push-entry',
    networkHost: options.networkHost,
  })
  if (options.shiftFocus) deps.setFocusedPane(pane)
  if (!options.terminal) scheduleVolumePathCorrection(deps, pane, token, volumeId, volumePath, targetPath)
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

  if ('location' in to) {
    return navigateToLocation(deps, intent, to.location)
  }

  return navigateToVolumeOrPath(deps, intent, to)
}

/**
 * The `{ location }` arm: go to a fully-resolved `(volumeId, path)`. Routes
 * itself — same volume as the pane → the in-place arm (commit-on-listing,
 * pinned-tab fork at `commitPathFromListing`, `push-path`); a different volume →
 * the switch arm (`commitVolumeSwitch`). The `=== current` test is safe HERE
 * (every `{ location }` caller is a genuine path navigation); it is NOT safe for
 * the volume-(re)select callers, which keep the always-switch `{ volumeId, path }`
 * arm precisely because they pass the CURRENT volume id to re-select it.
 */
function navigateToLocation(deps: NavigateDeps, intent: NavigateIntent, location: Location): NavigateResult {
  const { pane } = intent
  if (location.volumeId === deps.getPaneVolumeId(pane)) {
    return navigateInPlace(deps, intent, location.path)
  }
  return switchVolumeArm(deps, intent, location.volumeId, location.path)
}

/**
 * The switch arm shared by `{ location }` (cross-volume) and `{ volumeId, path }`
 * (deliberate volume-(re)select): mint a token, resolve the volume mount path,
 * and commit the optimistic synchronous switch. `'fallback'` is the edge-flow
 * recovery (MTP-fatal / retry / open-home / unmount): a terminal commit with no
 * old-path pre-save and no correction. The unmount redirect additionally
 * suppresses the history push (`pushHistory: false`).
 */
function switchVolumeArm(deps: NavigateDeps, intent: NavigateIntent, volumeId: string, path: string): NavigateResult {
  const { pane, source } = intent
  const token = mintToken(deps, pane)
  const volumePath = deps.getVolumePathById(volumeId) ?? path
  commitVolumeSwitch(deps, pane, token, volumeId, volumePath, path, {
    shiftFocus: shiftsFocus(source),
    terminal: source === 'fallback',
    pushHistory: intent.pushHistory,
  })
  return { status: 'started', settled: SETTLED_NOOP }
}

/**
 * The in-place path arm (same volume): the synchronous network / MTP refusals,
 * then drive the FilePane primitive. The commit — AND the pinned-tab fork (L7) —
 * land LATER, when the listing completes and the pane's `onPathChange` fires
 * `commitPathFromListing` (P4 — in-place arm is NOT optimistic). The fork lives
 * at the listing-completion landing, not here, because FilePane-INTERNAL
 * navigation (Enter on a folder, breadcrumb click) bypasses `navigate()` and
 * re-enters only through `onPathChange`; both entry paths must fork identically,
 * so the single fork point is `commitPathFromListing`. `settled` is the FilePane
 * promise. There is no cross-volume branch here: a `{ location }` whose volume
 * differs from the pane's took the switch arm instead.
 */
function navigateInPlace(deps: NavigateDeps, intent: NavigateIntent, path: string): NavigateResult {
  const { pane } = intent
  const currentVolumeId = deps.getPaneVolumeId(pane)
  const currentVolumeName = deps.getPaneVolumeName(pane)

  // On-network-volume refusal (synchronous).
  if (currentVolumeId === 'network') {
    return { status: 'refused', reason: onNetworkRefusal(currentVolumeName ?? currentVolumeId) }
  }

  // MTP capability refusal (synchronous).
  const mtpRefusal = validateMtpNavigation(path, currentVolumeId, currentVolumeName)
  if (mtpRefusal) return { status: 'refused', reason: mtpRefusal }

  const paneRef = deps.getPaneRef(pane)
  if (!paneRef) return { status: 'refused', reason: PANE_UNAVAILABLE_REFUSAL }
  return { status: 'started', settled: paneRef.navigateToPath(path, intent.selectName) }
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
    return switchVolumeArm(deps, intent, to.volumeId, to.path)
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
      // Swallow a `resolveVolume` rejection into a no-op, exactly as the old
      // `navigateToPath` cross-volume arm did with its try/catch. Without this,
      // a rejected resolve surfaces as an UNHANDLED promise rejection for the
      // fire-and-forget `mcp-nav-to-path` callers that never await `settled`
      // (e.g. the E2E `ensureAppReady` reset), which can freeze the webview.
      let result: { volume: { id: string; path: string } | null }
      try {
        result = await deps.resolveVolume(to.path)
      } catch {
        return
      }
      const volume = result.volume
      if (!volume) {
        // no-volume-resolved: today this logs + returns undefined (a no-op), NOT
        // a refusal. The MCP round-trip sees ok:true with no nav. Preserve that.
        return
      }
      if (!tokenLive(deps, pane, token)) return
      commitVolumeSwitch(deps, pane, token, volume.id, volume.path, to.path, { shiftFocus: shiftsFocus(source) })
    })()
    return { status: 'started', settled }
  }

  // MTP capability refusal (synchronous).
  const mtpRefusal = validateMtpNavigation(to.path, currentVolumeId, currentVolumeName)
  if (mtpRefusal) return { status: 'refused', reason: mtpRefusal }

  // In-place: drive the FilePane primitive. The commit — AND the pinned-tab fork
  // (L7) — land LATER, when the listing completes and the pane's onPathChange
  // fires `commitPathFromListing` (P4 — in-place arm is NOT optimistic). The fork
  // lives at the listing-completion landing, not here, because FilePane-INTERNAL
  // navigation (Enter on a folder, breadcrumb click) bypasses `navigate()` and
  // re-enters only through `onPathChange`; both entry paths must fork identically,
  // so the single fork point is `commitPathFromListing`. `settled` is the FilePane
  // promise.
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
 * The path commit landed from a FilePane `onPathChange` (folds `handlePathChange`
 * + `applyPathChange`, DPE:408/447). This is the SINGLE in-place fork+commit
 * point: BOTH coordinator-initiated in-place navs (`navigate({ to: { path } })`
 * drives the FilePane, which fires `onPathChange` on completion) AND
 * FilePane-internal navigations (Enter on a folder, breadcrumb click — they
 * bypass `navigate()` and re-enter only here) land here, so the pinned-tab fork
 * lives here, not in `navigate()`'s in-place arm.
 *
 * Order matches `handlePathChange`: the pinned fork is checked FIRST (before the
 * foreign-path drop), then the in-place commit applies the drop policy.
 *
 * **The drop policy (L6):** consults ONLY the foreign-path check — a virtual
 * volume checks its URL prefix, a real volume uses `isPathOnVolume`. The
 * transaction token is NOT checked here; it gates the background volume-path
 * correction (`scheduleVolumePathCorrection`), where a superseded `navigate()`
 * drops the stale correction. The drop-foreign-listings _policy_ is identical to
 * `applyPathChange` (L6); only the volumeChangeGeneration _mechanism_ is gone.
 *
 * **Token / self-re-entry semantics:** this is NOT a fresh `navigate()`; it does
 * NOT mint a token. A parent-nav (`{ history: 'parent' }`) or deleted-folder
 * walk-up re-enters here carrying the SAME transaction the in-place arm started —
 * no newer `navigate()` advanced the token, so the self-re-entry commits (it
 * isn't dropped as stale). A genuinely stale listing (the user flipped volumes ⇒
 * a path foreign to the new volume) is dropped by the foreign-path policy.
 *
 * Returns `true` iff an IN-PLACE commit happened (so the caller restores the
 * persisted cursor). A fork (new tab) or a dropped stale listing returns `false`:
 * neither restores the cursor (matching the old `handlePathChange` fork branch,
 * which returned before `applyPathChange`'s cursor restore).
 */
export function commitPathFromListing(deps: NavigateDeps, pane: 'left' | 'right', path: string): boolean {
  // Pinned-tab fork (L7) — checked FIRST, exactly as handlePathChange did. A
  // listing landing on a pinned tab at a NEW path opens a fresh unpinned tab
  // instead of navigating in-place. Returns `false`: a fork is not an in-place
  // commit, so the caller does not restore the cursor.
  if (commitPinnedPathFork(deps, pane, path)) return false

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

/**
 * The pinned-tab fork for an in-place path landing (the same-volume half of L7,
 * folded from `handlePathChange`'s pinned branch). When the active tab is pinned
 * and the landed `path` differs, open a NEW unpinned tab (same volume) carrying
 * the path; at `MAX_TABS_PER_PANE`, toast and fall through to in-place. Returns
 * `true` when a new tab was opened (the caller skips the in-place commit), `false`
 * to fall through to the in-place commit (not pinned, same path, or at cap).
 */
function commitPinnedPathFork(deps: NavigateDeps, pane: 'left' | 'right', path: string): boolean {
  const mgr = deps.getTabMgr(pane)
  const activeTab = getActiveTab(mgr)
  if (!activeTab.pinned || path === activeTab.path) return false

  if (mgr.tabs.length >= MAX_TABS_PER_PANE) {
    deps.addToast(tString('fileExplorer.tabs.limitReached'), { level: 'warn' })
    return false // fall through to in-place
  }

  // The fork stays on the SAME volume (an in-place path change on a pinned tab).
  spliceNewUnpinnedTab(mgr, activeTab, { volumeId: activeTab.volumeId, path })

  deps.persist({ kind: 'pane-state', pane })
  // Matches handlePathChange's pinned branch: record the new path as last-used for
  // the (unchanged) volume.
  deps.persist({ kind: 'last-used-path', record: { volumeId: activeTab.volumeId, path } })
  return true
}
