import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
// Static import so the eslint `no-isolated-tests` rule sees real source-code
// usage. The actual test cases dynamically re-import after stubbing `CSS`.
import * as webkitCompatModule from './webkit-compat'

// Sanity touch — also asserts the public API shape stays in sync.
void webkitCompatModule.hasColorMix
void webkitCompatModule.logWebkitCompat
void webkitCompatModule.meetsWebkitFloor
void webkitCompatModule.isBelowSupportedMacOs
void webkitCompatModule.SUPPORTED_MACOS_MAJOR

// We don't read `hasColorMix` from the static import in the cases because it's
// evaluated once at module load. Instead, each test stubs `CSS.supports`
// *before* a dynamic import, then reads the exported boolean.
// `vi.resetModules()` between tests forces a fresh evaluation per scenario.

// `vi.hoisted` is required because `vi.mock` is hoisted above the file's
// top-level `const` declarations; a plain reference would hit the TDZ.
const logSink = vi.hoisted(() => ({
  debug: vi.fn(),
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => logSink,
}))

beforeEach(() => {
  vi.resetModules()
  logSink.debug.mockClear()
  logSink.info.mockClear()
})

afterEach(() => {
  // @ts-expect-error - we deliberately reset the stub
  delete globalThis.CSS
})

describe('hasColorMix', () => {
  it('is true when CSS.supports reports color-mix()', async () => {
    globalThis.CSS = { supports: vi.fn(() => true) } as unknown as typeof CSS
    const mod = await import('./webkit-compat')
    expect(mod.hasColorMix).toBe(true)
  })

  it('is false when CSS.supports reports no color-mix()', async () => {
    globalThis.CSS = { supports: vi.fn(() => false) } as unknown as typeof CSS
    const mod = await import('./webkit-compat')
    expect(mod.hasColorMix).toBe(false)
  })

  it('falls back to true (assume modern) when CSS.supports is unavailable', async () => {
    // @ts-expect-error - simulate environments without `CSS.supports`
    delete globalThis.CSS
    const mod = await import('./webkit-compat')
    expect(mod.hasColorMix).toBe(true)
  })
})

describe('logWebkitCompat', () => {
  it('logs a debug line when color-mix() is supported', async () => {
    globalThis.CSS = { supports: vi.fn(() => true) } as unknown as typeof CSS
    const mod = await import('./webkit-compat')
    mod.logWebkitCompat()
    expect(logSink.debug).toHaveBeenCalledTimes(1)
    expect(logSink.info).not.toHaveBeenCalled()
  })

  it('logs an info line when color-mix() is unsupported', async () => {
    globalThis.CSS = { supports: vi.fn(() => false) } as unknown as typeof CSS
    const mod = await import('./webkit-compat')
    mod.logWebkitCompat()
    expect(logSink.info).toHaveBeenCalledTimes(1)
    expect(logSink.info.mock.calls[0][0]).toMatch(/Old WebKit/)
  })

  it('only logs once per session', async () => {
    globalThis.CSS = { supports: vi.fn(() => true) } as unknown as typeof CSS
    const mod = await import('./webkit-compat')
    mod.logWebkitCompat()
    mod.logWebkitCompat()
    mod.logWebkitCompat()
    expect(logSink.debug).toHaveBeenCalledTimes(1)
  })
})

describe('meetsWebkitFloor', () => {
  /** Puts a working stand-in for each Safari 15.4 capability on `globalThis`. */
  function stubModernWebkit() {
    globalThis.CSS = { supports: vi.fn(() => true) } as unknown as typeof CSS
    vi.stubGlobal('crypto', { randomUUID: () => '00000000-0000-4000-8000-000000000000' })
  }

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('is true on a WebKit that has every capability the app needs', async () => {
    stubModernWebkit()
    const mod = await import('./webkit-compat')
    expect(mod.meetsWebkitFloor).toBe(true)
  })

  it('is false without `crypto.randomUUID` (Safari 15.4)', async () => {
    stubModernWebkit()
    vi.stubGlobal('crypto', {})
    const mod = await import('./webkit-compat')
    expect(mod.meetsWebkitFloor).toBe(false)
  })

  it('is false without `:has()` support (Safari 15.4)', async () => {
    stubModernWebkit()
    globalThis.CSS = { supports: vi.fn((arg: string) => !arg.includes('selector(')) } as unknown as typeof CSS
    const mod = await import('./webkit-compat')
    expect(mod.meetsWebkitFloor).toBe(false)
  })

  it('assumes the floor is met when `CSS.supports` is missing entirely', async () => {
    // Same posture as `hasColorMix`: a webview that can't answer isn't evidence
    // against itself, and blocking the app on a missing probe would be worse
    // than the white screen we're preventing.
    stubModernWebkit()
    // @ts-expect-error - simulate an environment without `CSS.supports`
    delete globalThis.CSS
    const mod = await import('./webkit-compat')
    expect(mod.meetsWebkitFloor).toBe(true)
  })
})

describe('isBelowSupportedMacOs', () => {
  it('calls Catalina and Big Sur below the supported range', async () => {
    const mod = await import('./webkit-compat')
    // `get_macos_major_version` reports `10` on Catalina, not `10.15`.
    expect(mod.isBelowSupportedMacOs(10)).toBe(true)
    expect(mod.isBelowSupportedMacOs(11)).toBe(true)
  })

  it('calls Monterey and newer supported', async () => {
    const mod = await import('./webkit-compat')
    expect(mod.isBelowSupportedMacOs(12)).toBe(false)
    expect(mod.isBelowSupportedMacOs(26)).toBe(false)
  })

  it('treats an unreadable version as supported, so a probe that fails stays quiet', async () => {
    const mod = await import('./webkit-compat')
    expect(mod.isBelowSupportedMacOs(0)).toBe(false)
    expect(mod.isBelowSupportedMacOs(Number.NaN)).toBe(false)
  })
})

describe('macosVersionLabel', () => {
  it('writes Catalina as 10.15, the only 10.x the bundle can launch on', async () => {
    const mod = await import('./webkit-compat')
    expect(mod.macosVersionLabel(10)).toBe('10.15')
  })

  it('leaves every later release as its bare major', async () => {
    const mod = await import('./webkit-compat')
    expect(mod.macosVersionLabel(11)).toBe('11')
    expect(mod.macosVersionLabel(26)).toBe('26')
  })
})
