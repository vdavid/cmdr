/**
 * `navigate()`'s contract and its single commit point, shared by `navigate.ts` and
 * `navigate-return.ts` so neither imports the other: the intent, deps, result, and
 * persistence types, the one `commit` that writes volume, path, and history, and
 * transaction-token minting. What they mean, and the transaction itself: `navigate.ts`.
 */
import type { FilePaneAPI } from './types'
import { pushHistoryEntry, type TabManager } from '../tabs/tab-state-manager.svelte'
import { pushPath, setCurrentIndex, type NavigationHistory, type HistoryEntry } from '../navigation/navigation-history'
import type { DetermineNavigationPathArgs } from '../navigation/path-navigation'
import type { Location } from '$lib/tauri-commands'
import type { ReturnPoint } from './return-point'
import type { NavigateRefusal } from './navigate-refusals'

/** Where a navigation originates. Drives focus + history-push behavior, never the destination. */
export type NavigateSource = 'user' | 'mcp' | 'history' | 'correction' | 'cancel' | 'fallback' | 'mirror'

/**
 * The destination of a navigation: a `Location` (go somewhere), a deliberate
 * volume (re)select, a history walk, or a snapshot open. `Location` is
 * navigation's currency — a `(volumeId, path)` pair resolved at the four edges
 * (⌘G, MCP `nav_to_path`, search-result activation, downloads reveal) via
 * `navigation/resolve-location.ts`. `{ goTo }` routes itself: same volume →
 * in-place arm, different volume → switch arm. `{ selectVolume }` is the
 * deliberate volume-(re)select intent that ALWAYS takes the switch arm (its
 * callers legitimately pass the CURRENT volume id to re-select it).
 */
export type NavigateTo =
  | { goTo: Location } // navigate to a location; in-place arm (same volume) or switch arm (different volume)
  | { selectVolume: Location } // deliberately (re)activate a volume; ALWAYS the switch arm, even if already current
  | { history: 'back' | 'forward' | 'parent' }
  | { snapshot: string } // search-results snapshot id; routes through the volume-change machinery
  | { returnTo: ReturnPoint } // a cancelled load hands the pane back to what it showed

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

/**
 * The typed replacement for today's `navigateToPath` `string | Promise<void>`.
 * `started.settled` resolves when the listing completes (or per the per-arm
 * contract in `navigate.ts`); `refused` replaces the sync `string` sentinel three external
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
   * `{ moveTo }` points history back at an entry still in the stack (the
   * `{ returnTo }` arm).
   */
  history: 'push-path' | 'push-entry' | 'none' | { moveTo: number }
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
  /** The volume's mount path by id, or undefined when not in the live list. */
  getVolumePathById: (volumeId: string) => string | undefined
  /**
   * Where the volume lands when nothing is remembered about it, when that isn't
   * its root: a server place's start folder (`VolumeInfo.landingPath`). The
   * background correction takes it as its last default.
   */
  getVolumeLandingById: (volumeId: string) => string | null | undefined
  /** Whether a pane on this volume shows a listing: the Servers hub and a search snapshot don't. */
  volumeHasListing: (volumeId: string) => boolean
  /** Background "best path" resolver (`determineNavigationPath`), gated by the token. */
  determineNavigationPath: (args: DetermineNavigationPathArgs) => Promise<string>

  // --- side effects ---
  /** Persistence trigger fed to the single nav-state persistence subscriber (A5). */
  persist: (event: PersistEvent) => void
  /**
   * Warn toast (the `MAX_TABS_PER_PANE` "Tab limit reached" branch). Takes the
   * forking pane so the refusal is tagged to it and clears on that pane's next
   * navigation, not the other pane's.
   */
  addToast: (pane: 'left' | 'right', message: string, opts: { level: 'warn' }) => void

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
  /**
   * Per pane, what it showed before a navigation committed ahead of its listing
   * (`return-point.ts`). Caller-owned like `tokens`, and written only by `navigate.ts` and
   * `navigate-return.ts`: by a commit that runs ahead of its listing, by a landing, and by
   * the `{ returnTo }` arm.
   */
  returnPoints: Map<'left' | 'right', ReturnPoint>
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

/** A resolved no-op `settled` — for branches that commit state without driving a listing. */
export const SETTLED_NOOP: Promise<void> = Promise.resolve()

/**
 * Mints + stores a fresh transaction token for `pane`, returning it. Every fresh
 * `navigate()` call advances the pane's token; the self-re-entry path
 * (`commitPathFromListing`) deliberately does NOT call this.
 */
export function mintToken(deps: NavigateDeps, pane: 'left' | 'right'): number {
  const next = (deps.tokens.get(pane) ?? 0) + 1
  deps.tokens.set(pane, next)
  return next
}

/**
 * The single state-commit point. Writes volumeId (when switching) + path + an
 * optional history entry together, then fires the pane-state persistence intent.
 * This is the ONLY caller of `setPaneVolumeId` / `setPanePath` / `setPaneHistory`
 * outside the per-pane mutators themselves (the two orthogonal network-host
 * pushes aside).
 */
export function commit(deps: NavigateDeps, c: NavigateCommit): void {
  if (c.volumeId !== undefined) deps.setPaneVolumeId(c.pane, c.volumeId)
  deps.setPanePath(c.pane, c.path)

  if (c.history === 'push-path') {
    deps.setPaneHistory(c.pane, pushPath(deps.getPaneHistory(c.pane), c.path))
  } else if (c.history === 'push-entry') {
    const volumeId = c.volumeId ?? deps.getPaneVolumeId(c.pane)
    const entry: HistoryEntry = { volumeId, path: c.path }
    if (c.networkHost !== undefined) entry.networkHost = c.networkHost
    deps.setPaneHistory(c.pane, pushHistoryEntry(deps.getPaneHistory(c.pane), entry))
  } else if (c.history !== 'none') {
    deps.setPaneHistory(c.pane, setCurrentIndex(deps.getPaneHistory(c.pane), c.history.moveTo))
  }

  deps.persist({ kind: 'pane-state', pane: c.pane })
}
