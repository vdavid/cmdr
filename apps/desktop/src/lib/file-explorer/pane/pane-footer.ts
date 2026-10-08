/**
 * Which parts of the pane's status footer a given view gets.
 *
 * Two independent answers, because a search-results pane needs one and not the
 * other:
 * - `selectionInfo`: the row counts and the selection summary. A snapshot pane
 *   wants them as much as a folder does — "how many hits, how big are the ones I
 *   picked" is the whole question a search answers.
 * - `volumeSpace`: the free-space text and the usage bar above it. A snapshot
 *   isn't a place on a disk: its rows all live on ONE volume, but the pane isn't
 *   sitting IN that volume, so a free-space figure there would answer a question
 *   nobody asked and would follow the user around as they walk history.
 *
 * The network view has no listing to count, and an error state replaces the list
 * entirely, so both lose the footer completely.
 */

import type { PaneViewKind } from './types'

export interface PaneFooterInput {
  kind: PaneViewKind
  /** Whether the pane is showing an error or unreachable state instead of a list. */
  hasError: boolean
}

export interface PaneFooterVisibility {
  /** Render `SelectionInfo` under the list. */
  selectionInfo: boolean
  /** Let that readout talk about disk space, and render the usage bar above it. */
  volumeSpace: boolean
}

/** Resolves which footer pieces a pane view renders. */
export function paneFooterVisibility({ kind, hasError }: PaneFooterInput): PaneFooterVisibility {
  if (hasError) return { selectionInfo: false, volumeSpace: false }
  if (kind === 'normal') return { selectionInfo: true, volumeSpace: true }
  if (kind === 'search-results') return { selectionInfo: true, volumeSpace: false }
  return { selectionInfo: false, volumeSpace: false }
}
