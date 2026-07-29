import { describe, expect, it } from 'vitest'
import { coverageStrength } from './rename-evidence-coverage'
import type { RenameEvidenceCoverage } from '$lib/tauri-commands'

function coverage(matchedChars: number, deliveredChars: number): RenameEvidenceCoverage {
  return {
    matchOffset: 0,
    matchedChars,
    deliveredChars,
    contextBefore: '',
    matchedText: 'x'.repeat(matchedChars),
    contextAfter: '',
    trimmedBefore: false,
    trimmedAfter: false,
  }
}

describe('coverageStrength', () => {
  /** The case M1 exists for: a real quote that proves almost nothing about the file. */
  it('calls a sliver of a page of OCR thin', () => {
    expect(coverageStrength(coverage(7, 3_140))).toBe('thin')
    expect(coverageStrength(coverage(39, 2_000))).toBe('thin')
  })

  it('calls a quote that carries the text solid', () => {
    expect(coverageStrength(coverage(20, 61))).toBe('solid')
    expect(coverageStrength(coverage(40, 2_000))).toBe('solid')
  })

  /**
   * A short quote of a short text is normal, not thin: the excerpt beside it already shows
   * nearly everything Cmdr read, so there's nothing hidden to warn about.
   */
  it('never calls a short delivered text thin', () => {
    expect(coverageStrength(coverage(12, 199))).toBe('solid')
    expect(coverageStrength(coverage(12, 12))).toBe('solid')
  })
})
