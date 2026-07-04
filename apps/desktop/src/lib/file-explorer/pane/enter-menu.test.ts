import { describe, expect, it } from 'vitest'
import { buildEnterMenuItems, enterMenuAnchor, enterMenuHighlight } from './enter-menu'

describe('buildEnterMenuItems', () => {
  it('returns the three rows in order: browse, open, configure', () => {
    const items = buildEnterMenuItems()
    expect(items.map((i) => i.value)).toEqual(['browse', 'open', 'configure'])
    // Labels come from the catalog; assert they're non-empty (English fallback).
    for (const item of items) expect(item.label.length).toBeGreaterThan(0)
  })
})

describe('enterMenuHighlight', () => {
  it('leads with Open only when the resolved action is open', () => {
    expect(enterMenuHighlight('open')).toBe('open')
  })

  it('leads with Browse for browse and for ask (no preference)', () => {
    expect(enterMenuHighlight('browse')).toBe('browse')
    expect(enterMenuHighlight('ask')).toBe('browse')
  })
})

describe('enterMenuAnchor', () => {
  it('returns null when there is no pane element', () => {
    expect(enterMenuAnchor(null)).toBeNull()
  })

  it('anchors just below the left edge of the cursor row', () => {
    const row = { getBoundingClientRect: () => ({ left: 100, bottom: 40 }) }
    const paneEl = { querySelector: () => row } as unknown as HTMLElement
    expect(enterMenuAnchor(paneEl)).toEqual({ x: 116, y: 40 })
  })

  it('falls back to the pane center when no cursor row is found', () => {
    const paneEl = {
      querySelector: () => null,
      getBoundingClientRect: () => ({ left: 10, top: 20, width: 200, height: 100 }),
    } as unknown as HTMLElement
    expect(enterMenuAnchor(paneEl)).toEqual({ x: 110, y: 70 })
  })
})
