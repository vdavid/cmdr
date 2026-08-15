/**
 * Coverage honesty: what a search couldn't answer for, and how the dialog decides
 * whether to wait for an index at all.
 *
 * The backend answers every search with two TYPED sibling lists plus the volume it
 * routed to (`src-tauri/src/search/DETAILS.md` § Honesty). Both mean "this returned
 * nothing for a STRUCTURAL reason", and they mean different things, so they carry
 * distinct copy. Callers branch on emptiness, NEVER on message text.
 *
 * Pure: no state, no IPC. `SearchDialog.svelte` owns the state and the actions.
 */

import type { SearchResult, SearchRunCoverage, WalkEnding } from '$lib/tauri-commands'

/**
 * What a LIVE run couldn't cover: the extra half of the answer, once a search walks
 * what the index can't speak for instead of only reporting the gap.
 */
export interface LiveCoverage {
  /**
   * How the walk ended. `interrupted` and `cancelled` mean the list is a lower bound;
   * the status bar says that much, and the note says which of the two it was.
   */
  walk: WalkEnding
  /**
   * Folders a walk tried to read and was REFUSED, absolute. The half a user can act
   * on: on macOS it's usually Full Disk Access, and granting it heals the mark on
   * the next search.
   */
  permissionDenied: string[]
  /**
   * Folders no walk will read at all, by Cmdr's own choice: a NAS snapshot tree,
   * hardlinked per snapshot (44 TB reported on a 10 TB volume). Nothing for the
   * user to fix, so the copy explains rather than offers. ❌ Don't merge it into
   * `permissionDenied`: offering Full Disk Access over a snapshot folder is advice
   * that does nothing.
   */
  declined: string[]
  /**
   * Ground another search's walk is covering right now, so this run left it alone.
   * Those rows reach the same index, so this is "these arrive a bit later", ❌ never
   * "these are lost".
   */
  stillCovering: string[]
  /**
   * The walk gave up on folders it started: one that stopped responding and was
   * abandoned, or a subtree pruned after too many failed reads. TRUE means the
   * list is a lower bound even when `walk` is `completed` — the quiet third way a
   * run comes back short, alongside cancel and disconnect. Those folders stay
   * unlisted, so searching again retries them.
   */
  abandonedGround: boolean
  /**
   * How many PLACES that amounts to, folders grouped by their parent backend-side.
   * `0` alongside `abandonedGround` is real: this run's own walk gave up on ground
   * it recorded no path for, so the note says something was missed without saying
   * where. ❌ Never a folder count — a wedged mount marks thousands, and the number
   * would be true and useless.
   */
  abandonedLocations: number
}

/** What one run couldn't cover, ready to render. Absent when coverage was complete. */
export interface CoverageNote {
  /**
   * Scope paths whose volume has no search index at all (indexing declined, an
   * unindexed NAS share, an ejected drive). The drive is the thing to act on.
   */
  uncoveredScopes: string[]
  /**
   * Scope paths on an indexed volume that its index doesn't hold. PROVISIONAL: on a
   * partially indexed volume, a folder the user is standing in lands here just as a
   * typo does, so the copy says what Cmdr knows ("the index doesn't cover it") and
   * never that the folder doesn't exist. A live run reports its gaps through
   * `SearchRunCoverage` instead, which is what tells "not walked yet" from
   * "genuinely not found".
   */
  unresolvedScopes: string[]
  /** The volume the search covered, per the backend's routing. */
  volumeId: string
  /**
   * Present only for a live run. Its absence is what tells the note whether it's
   * speaking about an index-only answer (where an unindexed drive is a gap to offer to
   * fix) or a walked one (where the walk IS the fix, already applied).
   */
  live?: LiveCoverage
}

/**
 * The coverage note for a finished run, or `null` when the run covered everything it
 * was asked to. Both lists are checked independently: they're mutually exclusive
 * today by construction, and a note that assumed so would go quiet if that changed.
 */
export function coverageNoteFrom(result: SearchResult): CoverageNote | null {
  const uncoveredScopes = result.uncoveredScopes ?? []
  const unresolvedScopes = result.unresolvedScopes ?? []
  if (uncoveredScopes.length === 0 && unresolvedScopes.length === 0) return null
  return { uncoveredScopes, unresolvedScopes, volumeId: result.targetVolumeId ?? '' }
}

/**
 * The coverage note for a finished LIVE run, or `null` when it covered everything it
 * was asked to and finished doing so.
 *
 * There's no `uncoveredScopes` half here on purpose: a volume with no index used to be
 * the biggest gap a search could report, and a live run walks it instead. What's left
 * is what a walk genuinely can't answer for.
 */
export function coverageNoteFromRun(coverage: SearchRunCoverage): CoverageNote | null {
  const short = coverage.walk === 'interrupted' || coverage.walk === 'cancelled' || coverage.abandonedGround
  if (
    !short &&
    coverage.permissionDenied.length === 0 &&
    coverage.declined.length === 0 &&
    coverage.stillCovering.length === 0 &&
    coverage.unresolvedScopes.length === 0
  ) {
    return null
  }
  return {
    uncoveredScopes: [],
    unresolvedScopes: coverage.unresolvedScopes,
    volumeId: coverage.targetVolumeId,
    live: {
      walk: coverage.walk,
      permissionDenied: coverage.permissionDenied,
      declined: coverage.declined,
      stillCovering: coverage.stillCovering,
      abandonedGround: coverage.abandonedGround,
      abandonedLocations: coverage.abandonedLocations ?? 0,
    },
  }
}

/**
 * Whether the dialog may run a search now, or should keep waiting for an arena.
 *
 * The gate is PER TARGET, not "is root loaded". Only a volume with a pre-load in
 * flight is worth waiting for; everything else runs and lets the backend answer,
 * honestly and possibly with a coverage gap. That's what makes search reachable on a
 * machine that declined indexing, where nothing will ever report ready.
 *
 * `targetVolumeId` is `null` when the dialog can't know the target: the user typed a
 * scope, and routing a path to a volume is the backend's job (an SMB id keys on the
 * address; cloud drives route to `root`). Unknown means run — waiting on a guess is
 * how a search stops happening.
 */
export function isTargetIndexReady(input: {
  targetVolumeId: string | null
  /** Whether that volume's arena has landed. */
  isVolumeReady: (volumeId: string) => boolean
  /** The volume a background pre-load is in flight for; `null` when nothing is coming. */
  pendingVolumeId: string | null
}): boolean {
  if (input.targetVolumeId === null) return true
  if (input.isVolumeReady(input.targetVolumeId)) return true
  return input.pendingVolumeId !== input.targetVolumeId
}

/**
 * Whether the note should offer the Full Disk Access route.
 *
 * Three conditions, and dropping any one of them turns a helpful offer into a
 * misleading one:
 *
 * 1. **A folder was actually refused.** A run whose only unreadable ground is
 *    `declined` (a NAS snapshot tree) gets nothing: no permission on earth opens
 *    a folder Cmdr declines to read on purpose. This is what the typed cause on
 *    the wire buys — the paths alone can't tell the two apart, and matching folder
 *    names to guess isn't an option.
 * 2. **macOS.** Full Disk Access doesn't exist anywhere else; a Linux refusal is
 *    ordinary file permissions, which Cmdr can't grant itself either.
 * 3. **Cmdr doesn't already have it.** With Full Disk Access on, a refusal is a
 *    folder that belongs to someone else on the machine, and offering the setup
 *    that is already done would send the user somewhere that fixes nothing.
 *
 * The note still renders in every case: not offering a way out doesn't make the
 * gap untrue.
 */
export function offersFullDiskAccess(input: {
  note: CoverageNote | null
  /** `false` on Linux, where there is no such permission. */
  isMac: boolean
  /** What the side-effect-free TCC probe last said. */
  hasFullDiskAccess: boolean
}): boolean {
  if (!input.isMac || input.hasFullDiskAccess) return false
  return (input.note?.live?.permissionDenied.length ?? 0) > 0
}
