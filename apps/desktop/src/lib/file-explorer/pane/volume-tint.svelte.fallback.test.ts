/**
 * Coverage for the old-WebKit branch of `getPaneTintBg`.
 *
 * The default test file (`volume-tint.svelte.test.ts`) pins `hasColorMix` to
 * `true` so it can assert on the `color-mix(...)` string shape. This file
 * does the opposite: it forces the JS-mix branch and verifies the hex output.
 *
 * jsdom doesn't resolve CSS custom properties through `getComputedStyle`, so
 * we stub `getComputedStyle` to return the values `:root` would carry at
 * runtime. The resulting hex assertions only depend on the math in
 * `srgb-mix.ts`, which has its own tests.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

const subscribers = new Map<string, (id: string, value: unknown) => void>()
const settings = new Map<string, string>()

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((id: string) => settings.get(id) ?? 'none'),
  setSetting: vi.fn(),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn((id: string, cb: (id: string, value: unknown) => void) => {
    subscribers.set(id, cb)
    return () => subscribers.delete(id)
  }),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/utils/webkit-compat', () => ({
  hasColorMix: false,
  logWebkitCompat: vi.fn(),
}))

import { initVolumeTints, cleanupVolumeTints, getPaneTintBg } from './volume-tint.svelte'

const CSS_VARS: Record<string, string> = {
  '--color-bg-primary': '#ffffff',
  '--color-tint-blue': '#3b82f6',
  '--color-tint-red': '#ef4444',
  '--pane-tint-bg-pct': '90%',
  '--pane-tint-fg-pct': '10%',
}

let originalGetComputedStyle: typeof window.getComputedStyle

beforeEach(() => {
  settings.clear()
  subscribers.clear()
  // jsdom doesn't ship `matchMedia`; init wires it for the
  // `prefers-color-scheme` / `prefers-contrast` reactivity tick on the
  // old-WebKit branch, so stub a no-op.
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    value: vi.fn().mockImplementation(() => ({
      matches: false,
      media: '',
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      onchange: null,
      dispatchEvent: vi.fn(),
    })),
  })
  originalGetComputedStyle = window.getComputedStyle.bind(window)
  vi.spyOn(window, 'getComputedStyle').mockImplementation(() => {
    return {
      getPropertyValue: (name: string) => CSS_VARS[name] ?? '',
    } as unknown as CSSStyleDeclaration
  })
})

afterEach(() => {
  cleanupVolumeTints()
  vi.restoreAllMocks()
  window.getComputedStyle = originalGetComputedStyle
})

describe('getPaneTintBg (old-WebKit branch)', () => {
  it('returns null for the "none" tint', () => {
    initVolumeTints()
    expect(getPaneTintBg('root', 'apfs', 'main_volume')).toBeNull()
  })

  it('returns a hex string (not a color-mix() expression) for a configured tint', () => {
    settings.set('appearance.tintLocal', 'blue')
    initVolumeTints()
    const bg = getPaneTintBg('root', 'apfs', 'main_volume')
    expect(bg).toMatch(/^#[0-9a-f]{6}$/i)
    expect(bg).not.toContain('color-mix')
  })

  it('mixes bg + tint at the configured percentage', () => {
    settings.set('appearance.tintLocal', 'blue')
    initVolumeTints()
    // White (#ffffff) 90% + Tailwind blue-500 (#3b82f6) 10%:
    //   r = 255*0.9 + 59*0.1  = 235.4 → 0xeb
    //   g = 255*0.9 + 130*0.1 = 242.5 → 0xf3 (round-half-to-even: 0xf2 acceptable)
    //   b = 255*0.9 + 246*0.1 = 254.1 → 0xfe
    const bg = getPaneTintBg('root', 'apfs', 'main_volume')
    expect(bg).toBeTruthy()
    expect((bg ?? '').toLowerCase()).toMatch(/^#eb(f2|f3)fe$/)
  })

  it('returns null when CSS vars are not resolvable', () => {
    settings.set('appearance.tintLocal', 'blue')
    initVolumeTints()
    // Swap to a stub that returns empty strings (e.g. tint var unknown).
    vi.spyOn(window, 'getComputedStyle').mockImplementation(
      () =>
        ({
          getPropertyValue: () => '',
        }) as unknown as CSSStyleDeclaration,
    )
    expect(getPaneTintBg('root', 'apfs', 'main_volume')).toBeNull()
  })
})
