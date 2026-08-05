/**
 * Coverage honesty: what a search couldn't answer for, and how the dialog decides
 * whether to wait for an index at all.
 *
 * The backend answers every search with two TYPED sibling lists plus the volume it
 * routed to (`src-tauri/src/search/DETAILS.md` § Honesty). Both mean "this returned
 * nothing for a STRUCTURAL reason", and they mean different things, so they carry
 * distinct copy. Callers branch on emptiness, NEVER on message text
 * (`.claude/rules/no-string-matching.md`).
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
   * Folders nothing is going to read, absolute. TWO causes in ONE list, with nothing
   * on the wire to tell them apart: a folder the walk was refused (no Full Disk
   * Access), and a NAS snapshot tree the scanner declines on purpose (hardlinked per
   * snapshot, 44 TB reported on a 10 TB volume). ❌ Don't write copy that claims it's
   * one of them.
   */
  unreadable: string[]
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
   * never that the folder doesn't exist. M5 splits "not walked yet" from "genuinely
   * not found" once the walk can tell them apart
   * (`docs/specs/unindexed-search-plan.md`).
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
    coverage.unreadable.length === 0 &&
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
      unreadable: coverage.unreadable,
      stillCovering: coverage.stillCovering,
      abandonedGround: coverage.abandonedGround,
    },
  }
}

/**
 * Whether the dialog may run a search now, or should keep waiting for an arena.
 *
 * The gate is PER TARGET, not "is root loaded". Only a volume with a pre-load in
 * flight is worth waiting for; everything else runs and lets the backend answer,
 * honestly and possibly with a coverage gap. That's what makes search reachable on a
 * machine that declined indexing, where nothing will ever report ready
 * (`docs/specs/unindexed-search-plan.md` M1).
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
