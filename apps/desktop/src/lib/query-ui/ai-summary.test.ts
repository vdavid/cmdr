import { describe, it, expect } from 'vitest'
import { buildAiSummary, patternRowLabel, type AiSummaryInput } from './ai-summary'

function baseInput(overrides: Partial<AiSummaryInput> = {}): AiSummaryInput {
  return {
    pattern: null,
    patternKind: null,
    sizeFilter: 'any',
    sizeValue: '',
    sizeUnit: 'MB',
    sizeValueMax: '',
    sizeUnitMax: 'MB',
    dateFilter: 'any',
    dateValue: '',
    dateValueMax: '',
    typeFilter: 'both',
    ...overrides,
  }
}

describe('buildAiSummary', () => {
  it('surfaces the produced pattern verbatim with its kind', () => {
    const s = buildAiSummary(baseInput({ pattern: '*.{jpg,png,heic}', patternKind: 'glob' }))
    expect(s.pattern).toBe('*.{jpg,png,heic}')
    expect(s.patternKind).toBe('glob')
  })

  it('drops a blank pattern to null and clears the kind', () => {
    const s = buildAiSummary(baseInput({ pattern: '   ', patternKind: 'glob' }))
    expect(s.pattern).toBeNull()
    expect(s.patternKind).toBeNull()
  })

  it('renders the size filter line when configured', () => {
    const s = buildAiSummary(baseInput({ sizeFilter: 'gte', sizeValue: '5', sizeUnit: 'MB' }))
    expect(s.filters).toContainEqual({ label: 'Size', value: '> 5 MB' })
  })

  it('renders an exact (eq) size as "= N"', () => {
    const s = buildAiSummary(baseInput({ sizeFilter: 'eq', sizeValue: '0', sizeUnit: 'B' }))
    expect(s.filters).toContainEqual({ label: 'Size', value: '= 0 B' })
  })

  it('renders the modified filter line when configured', () => {
    const s = buildAiSummary(baseInput({ dateFilter: 'after', dateValue: '2026-01-01' }))
    expect(s.filters).toContainEqual({ label: 'Modified', value: 'after 2026-01-01' })
  })

  it('renders Files only / Folders only but omits the default Both', () => {
    expect(buildAiSummary(baseInput({ typeFilter: 'file' })).filters).toContainEqual({
      label: 'Type',
      value: 'Files only',
    })
    expect(buildAiSummary(baseInput({ typeFilter: 'folder' })).filters).toContainEqual({
      label: 'Type',
      value: 'Folders only',
    })
    expect(buildAiSummary(baseInput({ typeFilter: 'both' })).filters).toEqual([])
  })

  it('orders filters as Size, Modified, Type', () => {
    const s = buildAiSummary(
      baseInput({
        sizeFilter: 'gte',
        sizeValue: '1',
        sizeUnit: 'MB',
        dateFilter: 'before',
        dateValue: '2026-05-01',
        typeFilter: 'file',
      }),
    )
    expect(s.filters.map((f) => f.label)).toEqual(['Size', 'Modified', 'Type'])
  })

  it('returns no filter lines when nothing is configured', () => {
    expect(buildAiSummary(baseInput()).filters).toEqual([])
  })
})

describe('patternRowLabel', () => {
  it('names the flavor when known and falls back to Pattern', () => {
    expect(patternRowLabel('glob')).toBe('Glob')
    expect(patternRowLabel('regex')).toBe('Regex')
    expect(patternRowLabel(null)).toBe('Pattern')
  })
})
