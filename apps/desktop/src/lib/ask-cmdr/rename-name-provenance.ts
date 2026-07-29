/**
 * Where one proposed rename name came from, as the review dialog has to SHOW it.
 *
 * The evidence column names the source, but scanning 50 rows for the odd wrong one is the
 * actual review task, so "Cmdr read nothing inside this file" needs to be visible on the row
 * itself. That matters most for the path the model takes when it couldn't read a file at all:
 * it keeps a neutral name, and an instruction to do that is worthless if the user can't see
 * which rows took it.
 *
 * A display judgment over backend-supplied facts, like `rename-evidence-coverage.ts`: the
 * backend says what the name is based on, never whether the name is right.
 */

import type { RenameEvidenceSource } from '$lib/tauri-commands'

export type NameProvenance =
  /** Cmdr read what's inside the file (the backend verified the quote or the tags). */
  | 'contentRead'
  /** Nothing inside the file was read: the name rests on the old name, dates, or a request. */
  | 'nothingRead'
  /** Nothing was read AND the name didn't change, so Cmdr kept what the file already had. */
  | 'nameKept'
  /** The user typed this name in the review. It claims nothing, and needs to claim nothing. */
  | 'userEdited'

/** What to say about where this row's name came from. */
export function nameProvenance(row: {
  evidence: { source: RenameEvidenceSource }
  sourceName: string
  destinationName: string
}): NameProvenance {
  switch (row.evidence.source) {
    case 'imageText':
    case 'imageTags':
      return 'contentRead'
    case 'userEdited':
      return 'userEdited'
    case 'filename':
    case 'metadata':
    case 'userInstruction':
      // Exact comparison: a case-only edit IS a rename (the write engine stages it as one), so
      // it must not read as a kept name.
      return row.destinationName === row.sourceName ? 'nameKept' : 'nothingRead'
  }
}
