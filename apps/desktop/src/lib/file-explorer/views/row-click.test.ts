/**
 * What a click on a file row means for the selection, per OS: the platform's primary modifier
 * toggles the row (⌘ on macOS, Ctrl on Windows and Linux) and never arms click-to-rename. On macOS
 * Ctrl+click is the context menu, so it mustn't toggle there.
 */
import { describe, it, expect, beforeEach } from 'vitest'
import { blocksClickToRename, rowSelectPayload } from './row-click'
import { withPlatform } from '#lib/platform/test-platform.js'

function click(init: MouseEventInit = {}): MouseEvent {
  return new MouseEvent('mousedown', { button: 0, ...init })
}

describe.each(['windows', 'linux'] as const)('on %s', (platform) => {
  beforeEach(() => withPlatform(platform))

  it('Ctrl+click toggles the row and never renames', () => {
    expect(rowSelectPayload(click({ ctrlKey: true }), 3)).toEqual({ index: 3, shiftKey: false, toggle: true })
    expect(blocksClickToRename(click({ ctrlKey: true }))).toBe(true)
  })

  it('Super+click is a plain click: Super is the desktop’s key', () => {
    expect(rowSelectPayload(click({ metaKey: true }), 3)).toEqual({ index: 3, shiftKey: false, toggle: false })
    expect(blocksClickToRename(click({ metaKey: true }))).toBe(false)
  })
})

describe('on macos', () => {
  beforeEach(() => withPlatform('macos'))

  it('⌘+click toggles the row and never renames', () => {
    expect(rowSelectPayload(click({ metaKey: true }), 3)).toEqual({ index: 3, shiftKey: false, toggle: true })
    expect(blocksClickToRename(click({ metaKey: true }))).toBe(true)
  })

  it('Ctrl+click does not toggle (it opens the context menu)', () => {
    expect(rowSelectPayload(click({ ctrlKey: true }), 3)).toEqual({ index: 3, shiftKey: false, toggle: false })
  })
})

describe.each(['macos', 'windows', 'linux'] as const)('on %s, Shift', (platform) => {
  beforeEach(() => withPlatform(platform))

  it('extends the range and never renames', () => {
    expect(rowSelectPayload(click({ shiftKey: true }), 3)).toEqual({ index: 3, shiftKey: true, toggle: false })
    expect(blocksClickToRename(click({ shiftKey: true }))).toBe(true)
  })

  it('a plain click selects only, and may arm a rename', () => {
    expect(rowSelectPayload(click(), 3)).toEqual({ index: 3, shiftKey: false, toggle: false })
    expect(blocksClickToRename(click())).toBe(false)
  })
})
