/**
 * Tests for termbase write-back (`pnpm i18n:apply-terms`, `i18n-apply-terms-lib.ts`).
 * Fixture roots hold a tiny English catalog, a registry, and one locale's rulings.
 *
 * The load-bearing promises: a patch touches only what it names (order kept, a
 * new ID last, exceptions merged rather than replaced), a patch that adds a schema
 * problem is refused, and a shared `notMatch` finds the locale exceptions it makes stale.
 */
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { applyPatch, dropExceptionPatches, planTermsPatch, staleExceptionsUnder } from './i18n-apply-terms-lib.ts'
import { parseApplyTermsArgs } from './i18n-apply-terms.ts'
import type { Concepts } from './i18n-termbase-lib.ts'

let dir: string
let messagesRoot: string
let docsRoot: string

const writeJson = (path: string, value: unknown) => {
  writeFileSync(path, JSON.stringify(value, null, 2) + '\n', 'utf8')
}

const CONCEPTS = {
  window: { en: 'window', match: ['window', 'windows'], sense: 'An app window.' },
  move: { en: 'move', match: ['move', 'moving'], sense: 'Relocating files.' },
}

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'cmdr-i18n-apply-terms-'))
  messagesRoot = join(dir, 'messages')
  docsRoot = join(dir, 'docs')
  mkdirSync(join(messagesRoot, 'en'), { recursive: true })
  mkdirSync(join(messagesRoot, 'de'), { recursive: true })
  mkdirSync(join(docsRoot, 'de'), { recursive: true })
  writeJson(join(messagesRoot, 'en', 'a.json'), {
    'a.os': 'Windows has no safe way.',
    'a.close': 'Close the window.',
    'a.machine': 'After moving to a new PC.',
  })
  writeJson(join(messagesRoot, 'de', 'a.json'), { 'a.os': 'x', 'a.close': 'y', 'a.machine': 'z' })
  writeJson(join(docsRoot, 'concepts.json'), CONCEPTS)
  writeJson(join(docsRoot, 'de', 'terms.json'), {
    window: {
      chosen: 'Fenster',
      confidence: 'high',
      sources: 'macOS',
      exceptions: { 'a.os': 'the OS name' },
    },
    move: { chosen: 'bewegen', accept: ['beweg'], confidence: 'high', sources: 'Finder' },
  })
})

afterEach(() => {
  rmSync(dir, { recursive: true, force: true })
})

describe('applyPatch', () => {
  it('replaces named fields, deletes on null, merges exceptions, edits lists, and appends a new ID', () => {
    const out = applyPatch(
      {
        b: { chosen: 'x', accept: ['x1', 'x2'], note: 'old', exceptions: { k1: 'r1', k2: 'r2' } },
        a: { chosen: 'y' },
      },
      {
        b: { note: null, accept: { add: ['x3'], remove: ['x1'] }, exceptions: { k2: null, k3: 'r3' } },
        c: { chosen: 'z' },
      },
    )
    expect(Object.keys(out)).toEqual(['b', 'a', 'c'])
    expect(out.b).toEqual({ chosen: 'x', accept: ['x2', 'x3'], exceptions: { k1: 'r1', k3: 'r3' } })
  })

  it('removes an avoid entry by its form, and refuses to remove what is not there', () => {
    const file = { t: { avoid: [{ form: 'alt', why: 'old' }] } }
    expect(applyPatch(file, { t: { avoid: { remove: ['alt'] } } })).toEqual({ t: { avoid: [] } })
    expect(() => applyPatch(file, { t: { avoid: { remove: ['nope'] } } })).toThrow("isn't in the list")
    expect(() => applyPatch(file, { gone: null })).toThrow('no such entry')
  })
})

describe('planTermsPatch', () => {
  it('refuses a ruling for an unknown concept and one missing its required fields', () => {
    const plan = planTermsPatch(
      { kind: 'terms', tag: 'de' },
      { ghost: { chosen: 'x', confidence: 'high', sources: 's' }, move: { confidence: null } },
      { messagesRoot, docsRoot },
    )
    expect(plan.added.join('\n')).toContain('ghost: unknown concept')
    expect(plan.added.join('\n')).toContain('move')
  })

  it('accepts a clean ruling edit', () => {
    const plan = planTermsPatch(
      { kind: 'terms', tag: 'de' },
      { move: { accept: { add: ['verschieb'] } } },
      { messagesRoot, docsRoot },
    )
    expect(plan.added).toEqual([])
    expect((plan.content.move as { accept: string[] }).accept).toEqual(['beweg', 'verschieb'])
  })

  it('refuses a concept notMatch naming a key English lacks', () => {
    const plan = planTermsPatch(
      { kind: 'concepts' },
      { move: { notMatch: { add: [{ key: 'a.nope', why: 'x' }] } } },
      { messagesRoot, docsRoot },
    )
    expect(plan.added.join('\n')).toContain("isn't an English key")
  })
})

describe('stale exceptions after a shared notMatch', () => {
  it('finds the locale exceptions a notMatch makes stale, and the patch that drops them', () => {
    const plan = planTermsPatch(
      { kind: 'concepts' },
      { window: { notMatch: { add: ['windows has no'] } } },
      { messagesRoot, docsRoot },
    )
    expect(plan.added).toEqual([])
    const stale = staleExceptionsUnder(plan.content as Concepts, { messagesRoot, docsRoot })
    expect(stale).toEqual([{ tag: 'de', concept: 'window', key: 'a.os' }])

    const drop = dropExceptionPatches(stale).get('de')
    expect(drop).toEqual({ window: { exceptions: { 'a.os': null } } })
    const dropped = planTermsPatch({ kind: 'terms', tag: 'de' }, drop ?? {}, {
      messagesRoot,
      docsRoot,
      concepts: plan.content as Concepts,
    })
    expect(dropped.added).toEqual([])
    expect(dropped.content.window).toEqual({ chosen: 'Fenster', confidence: 'high', sources: 'macOS' })
  })
})

describe('parseApplyTermsArgs', () => {
  it('picks one target', () => {
    expect(parseApplyTermsArgs(['--lang', 'de', '--patch', 'p.json']).target).toEqual({ kind: 'terms', tag: 'de' })
    expect(parseApplyTermsArgs(['--concepts', '--patch', 'p.json']).target).toEqual({ kind: 'concepts' })
    expect(parseApplyTermsArgs(['--lang', 'de', '--proposed', '--patch', 'p.json']).target).toEqual({
      kind: 'proposed',
      tag: 'de',
    })
    expect(() => parseApplyTermsArgs(['--lang', 'de', '--concepts', '--patch', 'p.json'])).toThrow('exactly one')
    expect(() => parseApplyTermsArgs(['--lang', 'de', '--drop-stale-exceptions', '--patch', 'p.json'])).toThrow(
      '--concepts',
    )
  })
})
