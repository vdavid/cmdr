/**
 * Tests for measure-brief-column-widths.ts. Replaces pretext's measurer with
 * a deterministic `text.length * 7` stand-in for readable assertions.
 */
import { afterEach, describe, expect, it } from 'vitest'

import type { FileEntry } from '../types'

import { _setBriefMeasureForTests, measureWidestFilename } from './measure-brief-column-widths'

const fakeMeasure = (text: string): number => text.length * 7

function entry(name: string): FileEntry {
  return {
    name,
    path: `/x/${name}`,
    isDirectory: false,
    isSymlink: false,
    size: 0,
    permissions: 0o644,
    owner: 'u',
    group: 'g',
    iconId: 'text',
    extendedMetadataLoaded: false,
  }
}

describe('measureWidestFilename', () => {
  afterEach(() => {
    _setBriefMeasureForTests(null)
  })

  it('returns 0 when no measurer is available', () => {
    _setBriefMeasureForTests(null)
    // jsdom has no canvas, so the real measurer will set measureUnavailable=true
    // and return 0 — the caller should fall back to the cap width.
    expect(measureWidestFilename([entry('anything.txt')])).toBe(0)
  })

  it('returns the widest name across the column', () => {
    _setBriefMeasureForTests(fakeMeasure)
    const w = measureWidestFilename([entry('a.txt'), entry('longer.md'), entry('z')])
    expect(w).toBe('longer.md'.length * 7)
  })

  it('returns 0 for an empty column', () => {
    _setBriefMeasureForTests(fakeMeasure)
    expect(measureWidestFilename([])).toBe(0)
  })

  it('handles unicode names by character count via the fake measurer', () => {
    _setBriefMeasureForTests(fakeMeasure)
    const w = measureWidestFilename([entry('ábc'), entry('日本語.txt')])
    // "日本語.txt" has 7 code units (3 CJK + 4 ASCII)
    expect(w).toBe('日本語.txt'.length * 7)
  })
})
