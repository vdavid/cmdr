/**
 * Tests for the locale skeleton generator (`gen-locale-skeleton.ts`).
 *
 * The load-bearing case is the overlay refusal: the generator mirrors the WHOLE
 * `en` catalog, which is right for a new language and exactly wrong for a
 * regional variant, where every mirrored key would be a redundant override that
 * `desktop-i18n-coverage` then flags. Scaffolding is the one moment that mistake
 * is cheap to prevent.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, mkdirSync, rmSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { generateSkeleton } from './gen-locale-skeleton.ts'
import { sourceHash } from './i18n-catalog-lib.ts'

const EN = { 'app.trash': 'Move to Trash', '@app.trash': { description: 'Trash button' } }

let root: string

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-skeleton-'))
  mkdirSync(join(root, 'en'), { recursive: true })
  writeFileSync(join(root, 'en', 'app.json'), JSON.stringify(EN, null, 2) + '\n', 'utf8')
})
afterEach(() => {
  rmSync(root, { recursive: true, force: true })
})

describe('generateSkeleton', () => {
  it('mirrors en with a source hash per key, ready to translate in place', () => {
    expect(generateSkeleton('hu', { messagesRoot: root })).toEqual({ files: 1, keys: 1 })
    const written = JSON.parse(readFileSync(join(root, 'hu', 'app.json'), 'utf8')) as Record<string, unknown>
    expect(written['app.trash']).toBe('Move to Trash')
    expect(written['@app.trash']).toEqual({ sourceHash: sourceHash('Move to Trash') })
  })

  it('refuses the source locale', () => {
    expect(() => generateSkeleton('en', { messagesRoot: root })).toThrow(/source locale/)
  })

  it('refuses an OVERLAY tag, which must carry only its forks', () => {
    // `en` ships, so `en-GB` is an overlay: a full mirror would be 100% dead weight.
    expect(() => generateSkeleton('en-GB', { messagesRoot: root })).toThrow(/overlay/)
    expect(readdirSync(root).sort()).toEqual(['en'])
  })

  it('still scaffolds a variant whose language base does NOT ship', () => {
    expect(generateSkeleton('fr-CA', { messagesRoot: root })).toEqual({ files: 1, keys: 1 })
  })
})
