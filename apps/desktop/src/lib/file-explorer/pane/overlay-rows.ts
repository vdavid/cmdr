/**
 * Rows a listing OVERLAY adds beside a folder's real files: a restic repo's
 * `Browse snapshots` row and the git portal's six category rows in `.git/`.
 *
 * They're ways into a routed view, not files of the folder, so a "select all"
 * leaves them out the way it leaves out `..`: "select all, copy" in a repo
 * folder means the repo's real files. Copying the whole view too would be a
 * restore of every snapshot (tens of TB logical), and one batch spanning the
 * folder and the view reads through one source volume and fails
 * (`docs/notes/restic/conformance-review.md` § 2). A person can still pick the
 * row on its own.
 */

import { findFileIndices, getFileAt } from '#lib/tauri-commands/index.js'
import { getShowVirtualGitPortal } from '#lib/settings/reactive-settings.svelte.js'
import { VIRTUAL_GIT_CATEGORIES } from '../git/path-detection'
import { RESTIC_VIEW_FOLDER_NAME } from '../restic/path-detection'
import type { VolumeKind } from './volume-capabilities'
import { lastName } from '#lib/path/grammar.js'

/** The icon-id prefixes the backend reserves for view rows (`FileIcon.svelte` draws them). */
const VIEW_ROW_ICON_PREFIXES = ['restic:', 'git:'] as const

/**
 * The kinds with no overlay: panes inside a view already (every row there is the
 * view's content) and the two virtual kinds, which list no folder.
 */
const NO_OVERLAY_KINDS: ReadonlySet<VolumeKind> = new Set([
  'archive',
  'git-portal',
  'restic',
  'network',
  'search-results',
])

/**
 * The one predicate: a row an overlay added beside real files. It carries a
 * reserved view icon id (set by the backend on the overlay's rows, never on a
 * real folder), in a pane that isn't itself inside a view.
 */
export function isOverlayRow(entry: { iconId: string }, paneKind: VolumeKind): boolean {
  if (NO_OVERLAY_KINDS.has(paneKind)) return false
  return VIEW_ROW_ICON_PREFIXES.some((prefix) => entry.iconId.startsWith(prefix))
}

/** The names an overlay could add to a listing of `panePath`, so a lookup asks only for those. */
export function overlayRowCandidates(panePath: string, gitPortalOn: boolean): string[] {
  const names: string[] = [RESTIC_VIEW_FOLDER_NAME]
  if (gitPortalOn && lastName(panePath) === '.git') names.push(...VIRTUAL_GIT_CATEGORIES)
  return names
}

export interface OverlayRowLookup {
  listingId: string
  includeHidden: boolean
  panePath: string
  paneKind: VolumeKind
  gitPortalOn: boolean
  hasParent: boolean
}

/**
 * The PANE indices (past `..`) of the listing's overlay rows. One name lookup in
 * an ordinary folder, plus one row read per candidate found; nothing at all in a
 * pane inside a view.
 */
export async function findOverlayRowIndices(lookup: OverlayRowLookup): Promise<Set<number>> {
  const found = new Set<number>()
  if (NO_OVERLAY_KINDS.has(lookup.paneKind) || lookup.listingId === '') return found
  const byName = await findFileIndices(
    lookup.listingId,
    overlayRowCandidates(lookup.panePath, lookup.gitPortalOn),
    lookup.includeHidden,
  )
  const offset = lookup.hasParent ? 1 : 0
  for (const index of Object.values(byName)) {
    const entry = await getFileAt(lookup.listingId, index, lookup.includeHidden)
    if (entry && isOverlayRow(entry, lookup.paneKind)) found.add(index + offset)
  }
  return found
}

/**
 * `findOverlayRowIndices` for a pane's select-all, reading the live portal toggle.
 * A lookup that throws (the listing went away mid-call) skips nothing: the
 * backend refuses a batch that straddles a view anyway, so this only loses the
 * convenience, never safety.
 */
export async function overlayRowsToSkip(lookup: Omit<OverlayRowLookup, 'gitPortalOn'>): Promise<Set<number>> {
  try {
    return await findOverlayRowIndices({ ...lookup, gitPortalOn: getShowVirtualGitPortal() })
  } catch {
    return new Set()
  }
}
