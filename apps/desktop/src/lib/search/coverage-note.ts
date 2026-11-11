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

import type { SearchResult, SearchRunCoverage, WalkEnding } from '#lib/tauri-commands/index.js'

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
   * Folders a walk tried to read and macOS's privacy protection REFUSED, absolute.
   * The half a user can act on: granting Full Disk Access heals the mark on the next
   * search.
   */
  permissionDenied: string[]
  /**
   * Folders the OS refused for good: plain file permissions (root-only folders), SIP,
   * and every refusal off macOS. Shown, ❌ never offered Full Disk Access: nothing
   * the user can grant opens them (`crates/cmdr-index` `scanner/refusal.rs`).
   */
  offLimits: string[]
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
  /**
   * Folders the walk set out to read and couldn't open, with the drive still there
   * (the server didn't have that name, the folder went away, the listing failed).
   * Nothing inside them was read, so an empty list over one is ❌ never "no matches".
   * The backend keeps these out of `unresolvedScopes`, whose "press Enter" line
   * would offer a walk that just ran.
   */
  failedRoots: string[]
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
 * A volume with no index is walked rather than reported, so a live run's
 * `uncoveredScopes` is narrower than the one-shot answer's: only a volume NO index can
 * serve (a server), which nothing walks either. What's left besides is what a walk
 * genuinely can't answer for.
 */
export function coverageNoteFromRun(coverage: SearchRunCoverage): CoverageNote | null {
  const short = coverage.walk === 'interrupted' || coverage.walk === 'cancelled' || coverage.abandonedGround
  const uncoveredScopes = coverage.uncoveredScopes ?? []
  const failedRoots = coverage.failedRoots ?? []
  const offLimits = coverage.offLimits ?? []
  if (
    !short &&
    failedRoots.length === 0 &&
    coverage.permissionDenied.length === 0 &&
    offLimits.length === 0 &&
    coverage.declined.length === 0 &&
    coverage.stillCovering.length === 0 &&
    coverage.unresolvedScopes.length === 0 &&
    uncoveredScopes.length === 0
  ) {
    return null
  }
  return {
    uncoveredScopes,
    unresolvedScopes: coverage.unresolvedScopes,
    volumeId: coverage.targetVolumeId,
    live: {
      walk: coverage.walk,
      permissionDenied: coverage.permissionDenied,
      offLimits,
      declined: coverage.declined,
      stillCovering: coverage.stillCovering,
      abandonedGround: coverage.abandonedGround,
      abandonedLocations: coverage.abandonedLocations ?? 0,
      failedRoots,
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

/** Which list a coverage-note line is about. Each kind is its own sentence. */
export type CoverageLineKind =
  | 'failedRoots'
  | 'denied'
  | 'offLimits'
  | 'declined'
  | 'stillCovering'
  | 'uncovered'
  | 'unresolved'

/** One line of the note: a sentence, the folders behind its icon, and whether it asks anything of the reader. */
export interface CoverageLine {
  kind: CoverageLineKind
  /** Every folder the line is about, in the backend's order. Never empty. */
  paths: string[]
  /**
   * Whether the note offers a way out for THIS line (Full Disk Access, indexing the
   * drive, pressing Enter). Picks the icon: a warning where there's something to do,
   * an info glyph where there isn't. ❌ Never derived from the kind alone: a refused
   * folder is informational once Cmdr already has Full Disk Access.
   */
  actionable: boolean
}

/** The ways out the note is offering right now, as the dialog decided them. */
export interface CoverageOffers {
  /** The Full Disk Access setup is on screen (`offersFullDiskAccess`). */
  fullDiskAccess: boolean
  /** "Index this drive" is on screen. */
  indexDrive: boolean
  /** The "press Enter" line is on screen: an index-only answer on a drive Enter can walk. */
  pressEnter: boolean
}

/**
 * The note's lines, split into the two families the strip renders apart: ground
 * nothing will read (the abandoned-ground footnote sits under these), and ground this
 * run didn't get to. Empty lists produce no line.
 */
export function coverageLines(
  note: CoverageNote,
  offers: CoverageOffers,
): { unread: CoverageLine[]; unsearched: CoverageLine[] } {
  const live = note.live
  const line = (kind: CoverageLineKind, paths: string[] | undefined, actionable = false): CoverageLine[] =>
    paths && paths.length > 0 ? [{ kind, paths, actionable }] : []
  return {
    unread: [
      // First: a folder the walk set out to read is usually the scope itself, so it's
      // the whole reason the list is empty.
      ...line('failedRoots', live?.failedRoots),
      ...line('denied', live?.permissionDenied, offers.fullDiskAccess),
      ...line('offLimits', live?.offLimits),
      ...line('declined', live?.declined),
    ],
    unsearched: [
      ...line('stillCovering', live?.stillCovering),
      ...line('uncovered', note.uncoveredScopes, offers.indexDrive || offers.pressEnter),
      ...line('unresolved', note.unresolvedScopes, offers.pressEnter),
    ],
  }
}

/**
 * How many folders a line's tooltip lists before it says "+N more". The tooltip never
 * scrolls, so the cap is what keeps it on screen; the copy button carries the rest.
 */
export const TOOLTIP_FOLDER_CAP = 15

/** What a line's tooltip shows: the first folders, and how many it leaves to the copy button. */
export function tooltipFolders(paths: string[]): { shown: string[]; more: number } {
  const shown = paths.slice(0, TOOLTIP_FOLDER_CAP)
  return { shown, more: paths.length - shown.length }
}

/** What a line's copy button puts on the clipboard: every folder, one per line. */
export function foldersAsText(paths: string[]): string {
  return paths.join('\n')
}
