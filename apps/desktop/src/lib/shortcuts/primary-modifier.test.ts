import { describe, expect, it } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { holdsOnlyPrimaryModifier, holdsPrimaryModifier } from './primary-modifier'

function mods(init: Partial<Pick<KeyboardEvent, 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>>) {
  return { metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...init }
}

describe('on macos', () => {
  it('takes ⌘ alone as the primary modifier, never ⌃', () => {
    withPlatform('macos')
    expect(holdsPrimaryModifier(mods({ metaKey: true }))).toBe(true)
    expect(holdsPrimaryModifier(mods({ ctrlKey: true }))).toBe(false)
    expect(holdsOnlyPrimaryModifier(mods({ metaKey: true }))).toBe(true)
    expect(holdsOnlyPrimaryModifier(mods({ ctrlKey: true }))).toBe(false)
    expect(holdsOnlyPrimaryModifier(mods({ metaKey: true, ctrlKey: true }))).toBe(false)
    expect(holdsOnlyPrimaryModifier(mods({ metaKey: true, altKey: true }))).toBe(false)
    expect(holdsOnlyPrimaryModifier(mods({ metaKey: true, shiftKey: true }))).toBe(false)
  })
})

describe.each(['windows', 'linux'] as const)('on %s', (platform) => {
  it('takes Ctrl alone as the primary modifier, never Super', () => {
    withPlatform(platform)
    expect(holdsPrimaryModifier(mods({ ctrlKey: true }))).toBe(true)
    expect(holdsPrimaryModifier(mods({ metaKey: true }))).toBe(false)
    expect(holdsOnlyPrimaryModifier(mods({ ctrlKey: true }))).toBe(true)
    expect(holdsOnlyPrimaryModifier(mods({ metaKey: true }))).toBe(false)
    expect(holdsOnlyPrimaryModifier(mods({ ctrlKey: true, metaKey: true }))).toBe(false)
    expect(holdsOnlyPrimaryModifier(mods({ ctrlKey: true, altKey: true }))).toBe(false)
    expect(holdsOnlyPrimaryModifier(mods({ ctrlKey: true, shiftKey: true }))).toBe(false)
  })
})
