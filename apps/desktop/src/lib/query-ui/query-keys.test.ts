/**
 * The query dialog's per-platform key table: what each chip shows, what a screen reader hears,
 * and what the routers match, from one combo per platform.
 */
import { beforeEach, describe, expect, it } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { keydownWithAltGraph } from '#lib/shortcuts/test-key-events.js'
import { matchesQueryKey, queryKeyAria, queryKeyLabel } from './query-keys'

function key(init: KeyboardEventInit): KeyboardEvent {
  return new KeyboardEvent('keydown', init)
}

describe('query keys on macos', () => {
  beforeEach(() => withPlatform('macos'))

  it('shows ⌥ glyphs, and Enter as ⏎ as the dialog always has', () => {
    expect(queryKeyLabel('modeFilename')).toBe('⌥F')
    expect(queryKeyLabel('scopeVolume')).toBe('⌥V')
    expect(queryKeyLabel('primaryAction')).toBe('⌥⏎')
    expect(queryKeyLabel('enter')).toBe('⏎')
    expect(queryKeyAria('modeFilename')).toBe('Alt+F')
    expect(queryKeyAria('primaryAction')).toBe('Alt+Enter')
  })

  it('matches an ⌥ letter by key position, whatever glyph ⌥ composed', () => {
    expect(matchesQueryKey(key({ key: 'ß', code: 'KeyS', altKey: true }), 'sizeFilter')).toBe(true)
    expect(matchesQueryKey(key({ key: 'µ', code: 'KeyM', altKey: true }), 'dateFilter')).toBe(true)
    expect(matchesQueryKey(key({ key: 's', altKey: true }), 'sizeFilter')).toBe(true)
  })

  it('rejects supersets', () => {
    expect(matchesQueryKey(key({ key: 'ß', code: 'KeyS', altKey: true, metaKey: true }), 'sizeFilter')).toBe(false)
    expect(matchesQueryKey(key({ key: 'Í', code: 'KeyS', altKey: true, shiftKey: true }), 'sizeFilter')).toBe(false)
    expect(matchesQueryKey(key({ key: 'Enter', altKey: true, metaKey: true }), 'primaryAction')).toBe(false)
  })
})

describe.each(['windows', 'linux'] as const)('query keys on %s', (platform) => {
  beforeEach(() => withPlatform(platform))

  it('shows Alt+ and Enter, never a Mac glyph', () => {
    expect(queryKeyLabel('modeAi')).toBe('Alt+A')
    expect(queryKeyLabel('scopeCurrentFolder')).toBe('Alt+C')
    expect(queryKeyLabel('primaryAction')).toBe('Alt+Enter')
    expect(queryKeyLabel('enter')).toBe('Enter')
    expect(queryKeyAria('scopeCurrentFolder')).toBe('Alt+C')
  })

  it('matches Alt+<letter>, also on a non-Latin layout, but not with Ctrl or Super', () => {
    expect(matchesQueryKey(key({ key: 'i', code: 'KeyI', altKey: true }), 'scopeFilter')).toBe(true)
    expect(matchesQueryKey(key({ key: 'ш', code: 'KeyI', altKey: true }), 'scopeFilter')).toBe(true)
    expect(matchesQueryKey(key({ key: 'i', code: 'KeyI', altKey: true, ctrlKey: true }), 'scopeFilter')).toBe(false)
    expect(matchesQueryKey(key({ key: 'i', code: 'KeyI', altKey: true, metaKey: true }), 'scopeFilter')).toBe(false)
  })

  it('leaves an AltGr-typed character to the field', () => {
    // Polish AltGr+S types `ś`; Windows reports it as Ctrl+Alt, Linux sets neither flag.
    const windowsShape = keydownWithAltGraph({ key: 'ś', code: 'KeyS', altKey: true, ctrlKey: true })
    const linuxShape = keydownWithAltGraph({ key: 'ś', code: 'KeyS' })
    expect(matchesQueryKey(windowsShape, 'sizeFilter')).toBe(false)
    expect(matchesQueryKey(linuxShape, 'sizeFilter')).toBe(false)
  })
})
