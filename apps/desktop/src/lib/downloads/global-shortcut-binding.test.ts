import { describe, it, expect, onTestFinished } from 'vitest'
import { PLATFORMS } from '#lib/commands/platform-shortcuts.js'
import { withPlatform } from '#lib/platform/test-platform.js'
import { getDefaultValue } from '#lib/settings/index.js'
import { formatKeyCombo } from '#lib/shortcuts/key-capture.js'
import { toAccelerator } from './global-shortcut-binding'
import { GLOBAL_GO_TO_LATEST_BINDING_KEY } from './global-shortcut-setting'

describe('toAccelerator', () => {
  it('translates the macOS spelling ⌃⌥⌘J to Control+Alt+Super+J', () => {
    expect(toAccelerator('⌃⌥⌘J')).toBe('Control+Alt+Super+J')
  })

  it('translates the Windows and Linux spelling Ctrl+Alt+Super+J to the same accelerator', () => {
    expect(toAccelerator('Ctrl+Alt+Super+J')).toBe('Control+Alt+Super+J')
    expect(toAccelerator('Ctrl+Shift+K')).toBe('Control+Shift+K')
  })

  it('translates a Cmd+Shift+K combo', () => {
    expect(toAccelerator('⌘⇧K')).toBe('Super+Shift+K')
  })

  it('uppercases the key half', () => {
    expect(toAccelerator('⌘j')).toBe('Super+J')
  })

  it('keeps a key that looks like a modifier token’s tail', () => {
    expect(toAccelerator('Ctrl++')).toBe('Control++')
  })

  it('returns null for empty input', () => {
    expect(toAccelerator('')).toBeNull()
  })

  it('returns null for a binding with no modifiers (global shortcut requires one)', () => {
    expect(toAccelerator('J')).toBeNull()
  })

  it('returns null for a binding with only modifiers, no key', () => {
    expect(toAccelerator('⌘⇧')).toBeNull()
    expect(toAccelerator('Ctrl+Alt')).toBeNull()
  })

  it('deduplicates accidentally-repeated modifiers', () => {
    // Hand-typed pathological case; the recorder shouldn't emit this but we
    // shouldn't choke on it either.
    expect(toAccelerator('⌘⌘K')).toBe('Super+K')
  })
})

// The same physical chord on every OS for now, each spelled its own way.
const EXPECTED_DEFAULT: Record<(typeof PLATFORMS)[number], string> = {
  macos: '⌃⌥⌘J',
  windows: 'Ctrl+Alt+Super+J',
  linux: 'Ctrl+Alt+Super+J',
}

describe.each(PLATFORMS)('the default binding on %s', (platform) => {
  it('is spelled the OS’s own way and registers as Control+Alt+Super+J', () => {
    onTestFinished(withPlatform(platform))
    const binding = getDefaultValue(GLOBAL_GO_TO_LATEST_BINDING_KEY)

    expect(binding).toBe(EXPECTED_DEFAULT[platform])
    expect(toAccelerator(binding)).toBe('Control+Alt+Super+J')
  })
})

// Off macOS the default is exactly what the recorder writes for the chord, so recording it again
// reads as unmodified. macOS keeps Apple's menu order (`⌃⌥⌘J`), which its recorder spells `⌘⌃⌥J`.
describe.each(['windows', 'linux'] as const)('recording the default chord on %s', (platform) => {
  it('records the default binding itself', () => {
    onTestFinished(withPlatform(platform))
    const pressed = { ctrlKey: true, altKey: true, metaKey: true, shiftKey: false, key: 'j', code: 'KeyJ' }

    expect(formatKeyCombo(pressed as KeyboardEvent)).toBe(getDefaultValue(GLOBAL_GO_TO_LATEST_BINDING_KEY))
  })
})
