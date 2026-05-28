/**
 * Pins the Search-only extras shape and the AI-write split contract: the core's
 * `recordAiTranslation` writes ONLY to `handTyped[mode]`; the extras module's
 * `recordAiPatternAndLabel` writes ONLY to its own AI fields. Calling both in
 * sequence (Search's wrapper does) leaves the two surfaces in sync.
 */
import { describe, it, expect } from 'vitest'
import { createQueryFilterState } from '$lib/query-ui/query-filter-state.svelte'
import { createSearchExtrasState } from './search-extras-state.svelte'

describe('createSearchExtrasState: defaults + setters', () => {
  it('starts with empty scope, excludeSystemDirs on, no AI pattern / label', () => {
    const e = createSearchExtrasState()
    expect(e.getScope()).toBe('')
    expect(e.getExcludeSystemDirs()).toBe(true)
    expect(e.getLastAiLabel()).toBeNull()
    expect(e.getLastAiPattern()).toBeNull()
    expect(e.getLastAiPatternKind()).toBeNull()
  })

  it('stores scope and the system-dirs flag', () => {
    const e = createSearchExtrasState()
    e.setScope('~/projects, !node_modules')
    e.setExcludeSystemDirs(false)
    expect(e.getScope()).toBe('~/projects, !node_modules')
    expect(e.getExcludeSystemDirs()).toBe(false)
  })
})

describe('createSearchExtrasState: factory isolation', () => {
  it('two instances do not share state', () => {
    const a = createSearchExtrasState()
    const b = createSearchExtrasState()
    a.setScope('~/a')
    b.setScope('~/b')
    expect(a.getScope()).toBe('~/a')
    expect(b.getScope()).toBe('~/b')
    a.recordAiPatternAndLabel({ pattern: '*.pdf', kind: 'glob', label: 'PDFs' })
    expect(a.getLastAiPattern()).toBe('*.pdf')
    expect(b.getLastAiPattern()).toBeNull()
  })
})

describe('createSearchExtrasState: recordAiPatternAndLabel', () => {
  it('stores pattern + kind + label together', () => {
    const e = createSearchExtrasState()
    e.recordAiPatternAndLabel({ pattern: '*.pdf', kind: 'glob', label: 'PDFs from this week' })
    expect(e.getLastAiPattern()).toBe('*.pdf')
    expect(e.getLastAiPatternKind()).toBe('glob')
    expect(e.getLastAiLabel()).toBe('PDFs from this week')
  })

  it('blanks the kind when pattern is null', () => {
    const e = createSearchExtrasState()
    e.recordAiPatternAndLabel({ pattern: '*.pdf', kind: 'glob', label: 'PDFs' })
    e.recordAiPatternAndLabel({ pattern: null, kind: 'glob', label: null })
    expect(e.getLastAiPattern()).toBeNull()
    expect(e.getLastAiPatternKind()).toBeNull()
  })

  it('clearAiPattern wipes pattern + kind but leaves label and other extras alone', () => {
    const e = createSearchExtrasState()
    e.recordAiPatternAndLabel({ pattern: 'foo.*', kind: 'regex', label: 'A label' })
    e.setScope('~/somewhere')
    e.clearAiPattern()
    expect(e.getLastAiPattern()).toBeNull()
    expect(e.getLastAiPatternKind()).toBeNull()
    // Label intentionally stays — only pattern + kind clear.
    expect(e.getLastAiLabel()).toBe('A label')
    expect(e.getScope()).toBe('~/somewhere')
  })
})

describe('createSearchExtrasState: clearExtras', () => {
  it('resets every extras field to defaults', () => {
    const e = createSearchExtrasState()
    e.setScope('something')
    e.setExcludeSystemDirs(false)
    e.recordAiPatternAndLabel({ pattern: '*.pdf', kind: 'glob', label: 'PDFs' })
    e.clearExtras()
    expect(e.getScope()).toBe('')
    expect(e.getExcludeSystemDirs()).toBe(true)
    expect(e.getLastAiLabel()).toBeNull()
    expect(e.getLastAiPattern()).toBeNull()
    expect(e.getLastAiPatternKind()).toBeNull()
  })
})

describe('split contract: core.recordAiTranslation + extras.recordAiPatternAndLabel stay in sync', () => {
  // Core owns `handTyped[mode]`; extras owns `lastAiPattern` + `Kind` + `Label`.
  // Calling both in sequence (Search's wrapper does this) must leave both in sync.

  it('core writes only to the matching hand-typed buffer (handTyped.filename for glob)', () => {
    const core = createQueryFilterState()
    const extras = createSearchExtrasState()
    core.recordAiTranslation({ pattern: '*.pdf', kind: 'glob' })
    expect(core.getHandTypedBuffer('filename')).toBe('*.pdf')
    // Crucially: nothing touched the extras side.
    expect(extras.getLastAiPattern()).toBeNull()
    expect(extras.getLastAiLabel()).toBeNull()
    expect(extras.getLastAiPatternKind()).toBeNull()
  })

  it('extras writes only to its own AI fields (pattern + kind + label)', () => {
    const core = createQueryFilterState()
    const extras = createSearchExtrasState()
    extras.recordAiPatternAndLabel({ pattern: '*.pdf', kind: 'glob', label: 'PDFs' })
    expect(extras.getLastAiPattern()).toBe('*.pdf')
    expect(extras.getLastAiPatternKind()).toBe('glob')
    expect(extras.getLastAiLabel()).toBe('PDFs')
    // Crucially: nothing touched the core side.
    expect(core.getHandTypedBuffer('filename')).toBe('')
    expect(core.getHandTypedBuffer('regex')).toBe('')
  })

  it('calling both in sequence (Search wrapper order) leaves both in sync', () => {
    const core = createQueryFilterState()
    const extras = createSearchExtrasState()

    const input = { pattern: '*.pdf', kind: 'glob' as const, label: 'PDFs from this week' }
    core.recordAiTranslation({ pattern: input.pattern, kind: input.kind })
    extras.recordAiPatternAndLabel(input)

    expect(core.getHandTypedBuffer('filename')).toBe('*.pdf')
    expect(extras.getLastAiPattern()).toBe('*.pdf')
    expect(extras.getLastAiPatternKind()).toBe('glob')
    expect(extras.getLastAiLabel()).toBe('PDFs from this week')
  })

  it('Selection wrapper order: core only, no extras call', () => {
    // Selection's wrapper doesn't compose extras at all. Make sure the core
    // method works standalone without leaving Selection-irrelevant fields in
    // an odd state.
    const core = createQueryFilterState()
    core.recordAiTranslation({ pattern: 'foo.*', kind: 'regex' })
    expect(core.getHandTypedBuffer('regex')).toBe('foo.*')
    // The switchMode flow still works (probe stays null by default).
    core.setMode('ai')
    core.switchMode('regex')
    expect(core.getQuery()).toBe('foo.*')
  })
})
