import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
// Static import so the eslint `no-isolated-tests` rule sees real source-code
// usage. The actual test cases dynamically re-import after stubbing `CSS`.
import * as webkitCompatModule from './webkit-compat'

// Sanity touch — also asserts the public API shape stays in sync.
void webkitCompatModule.hasColorMix
void webkitCompatModule.logWebkitCompat

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
