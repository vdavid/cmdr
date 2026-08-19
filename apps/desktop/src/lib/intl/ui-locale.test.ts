/**
 * What `appearance.language: 'system'` resolves to, and how the OS answer is
 * fetched.
 *
 * Two properties matter here. The `'system'` sentinel must resolve through the
 * OS answer rather than the webview's single tag (the whole reason the resolver
 * lives in Rust), and the fetch must never be able to take the app down: a
 * failed read leaves the webview default standing.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const getUiLocale = vi.fn<() => Promise<string | null>>()

vi.mock('$lib/tauri-commands', () => ({
  getUiLocale: () => getUiLocale(),
}))

import { loadSystemUiLocale, pickUiLocale, _setSystemUiLocaleForTests } from './ui-locale'

beforeEach(() => {
  getUiLocale.mockReset()
  _setSystemUiLocaleForTests(null)
})

describe('pickUiLocale', () => {
  it('hands back an explicit language untouched', () => {
    expect(pickUiLocale('hu')).toBe('hu')
  })

  it('resolves the `system` sentinel through the OS answer', () => {
    _setSystemUiLocaleForTests('sv')
    expect(pickUiLocale('system')).toBe('sv')
  })

  it('answers null for `system` before the OS answer arrives', () => {
    // `null` is "no override": the webview default stands, which is a
    // reasonable language rather than none at all.
    expect(pickUiLocale('system')).toBeNull()
  })
})

describe('loadSystemUiLocale', () => {
  it('costs one IPC round-trip however many callers ask', async () => {
    getUiLocale.mockResolvedValue('de')
    const [first, second] = await Promise.all([loadSystemUiLocale(), loadSystemUiLocale()])
    expect(first).toBe('de')
    expect(second).toBe('de')
    expect(getUiLocale).toHaveBeenCalledTimes(1)
    expect(pickUiLocale('system')).toBe('de')
  })

  it('leaves the webview default standing when the read fails, without throwing', async () => {
    getUiLocale.mockRejectedValue(new Error('no such command'))
    await expect(loadSystemUiLocale()).resolves.toBeNull()
    expect(pickUiLocale('system')).toBeNull()
  })

  it('normalizes a missing answer to null, so callers get one shape', async () => {
    // Off macOS the command answers `null`; a stubbed IPC layer can answer
    // `undefined`. Both mean the same thing to `setLocale()`.
    getUiLocale.mockResolvedValue(undefined as unknown as null)
    await expect(loadSystemUiLocale()).resolves.toBeNull()
  })
})
