/**
 * How strong the quote behind a proposed rename actually is, for the review dialog's
 * "Why this name" column.
 *
 * The backend supplies the honest counts (`RenameEvidenceCoverage`: where the quote sits in the
 * text Cmdr read in the image, how long it is, how much text there was). Turning those into
 * "thin" or "solid" is a DISPLAY judgment, so it lives here, beside the column that renders it,
 * and never in the guardrail: evidence validation proves the model READ something, it can never
 * prove the name is right. A thin match is flagged for the human, never refused.
 */

import type { RenameEvidenceCoverage } from '$lib/tauri-commands'

/**
 * Under this share of the delivered text, a quote is a sliver rather than the gist: it proves
 * the model read a line, not that it understood the file.
 */
const THIN_COVERAGE_RATIO = 0.02

/**
 * Below this much delivered text there is no "buried in a page of OCR" to warn about. A short
 * quote of a short text is normal, and the excerpt beside it already shows nearly everything.
 */
const MIN_DELIVERED_CHARS_FOR_THIN = 200

export type CoverageStrength = 'thin' | 'solid'

/** Whether this match is a sliver of a long text, so the row can look as thin as it is. */
export function coverageStrength(coverage: RenameEvidenceCoverage): CoverageStrength {
  if (coverage.deliveredChars < MIN_DELIVERED_CHARS_FOR_THIN) return 'solid'
  return coverage.matchedChars / coverage.deliveredChars < THIN_COVERAGE_RATIO ? 'thin' : 'solid'
}
