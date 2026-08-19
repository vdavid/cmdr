/**
 * The OS's two locale answers: how they're fetched, what `'system'` resolves
 * to, and how a live change reaches the app.
 *
 * Three properties matter here. The `'system'` sentinel must resolve through
 * the OS answer rather than the webview's single tag (the whole reason the
 * resolver lives in Rust); the formatting tag must reach `getFormatLocale()`
 * the moment it arrives, because the webview's own locale is missing the user's
 * region; and the fetch must never be able to take the app down, since a failed
 * read leaves a working webview default standing.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { OsLocales } from '$lib/ipc/bindings'

const getOsLocales = vi.fn<() => Promise<OsLocales>>()

/** The handler `watchSystemLocales` registered, so a test can post an event. */
let onEvent: ((payload: { locales: OsLocales }) => void) | undefined
const unlisten = vi.fn()

vi.mock('$lib/tauri-commands', () => ({
  getOsLocales: () => getOsLocales(),
  onOsLocalesChanged: (handler: (payload: { locales: OsLocales }) => void) => {
    onEvent = handler
    return Promise.resolve(unlisten)
  },
}))

import { loadSystemLocales, pickUiLocale, watchSystemLocales, _setSystemLocalesForTests } from './os-locales'
import { getFormatLocale } from './locale'

/** The webview's own locale: what stands in when the OS doesn't answer. */
const webviewLocale = new Intl.NumberFormat().resolvedOptions().locale

beforeEach(() => {
  getOsLocales.mockReset()
  onEvent = undefined
  unlisten.mockClear()
  _setSystemLocalesForTests({ ui: null, format: null })
})

describe('pickUiLocale', () => {
  it('hands back an explicit language untouched', () => {
    expect(pickUiLocale('hu')).toBe('hu')
  })

  it('resolves the `system` sentinel through the OS answer', () => {
    _setSystemLocalesForTests({ ui: 'sv', format: 'sv-SE' })
    expect(pickUiLocale('system')).toBe('sv')
  })

  it('answers null for `system` before the OS answer arrives', () => {
    // `null` is "no override": the webview default stands, which is a
    // reasonable language rather than none at all.
    expect(pickUiLocale('system')).toBeNull()
  })
})

describe('loadSystemLocales', () => {
  it('costs one IPC round-trip however many callers ask', async () => {
    getOsLocales.mockResolvedValue({ ui: 'de', format: 'de-DE' })

    const [first, second] = await Promise.all([loadSystemLocales(), loadSystemLocales()])

    expect(first).toEqual({ ui: 'de', format: 'de-DE' })
    expect(second).toEqual({ ui: 'de', format: 'de-DE' })
    expect(getOsLocales).toHaveBeenCalledTimes(1)
    expect(pickUiLocale('system')).toBe('de')
  })

  it('hands the composed tag to the formatters, region and all', async () => {
    // The motivating machine: US English, Swedish region. Nothing downstream
    // asks for the tag; adopting it here is what makes every formatter follow.
    getOsLocales.mockResolvedValue({ ui: 'en', format: 'en-SE' })

    await loadSystemLocales()

    expect(getFormatLocale()).toBe('en-SE')
  })

  it('leaves the webview default standing when the read fails, without throwing', async () => {
    getOsLocales.mockRejectedValue(new Error('no such command'))

    await expect(loadSystemLocales()).resolves.toEqual({ ui: null, format: null })
    expect(pickUiLocale('system')).toBeNull()
    expect(getFormatLocale()).toBe(webviewLocale)
  })

  it('normalizes a missing answer to nulls, so callers get one shape', async () => {
    // Off macOS the command answers nulls; a stubbed IPC layer can answer
    // nothing at all. Both mean the same thing downstream.
    getOsLocales.mockResolvedValue(undefined as unknown as OsLocales)

    await expect(loadSystemLocales()).resolves.toEqual({ ui: null, format: null })
    expect(getFormatLocale()).toBe(webviewLocale)
  })
})

describe('watchSystemLocales', () => {
  it('adopts a moved language, so `system` re-resolves without a restart', async () => {
    _setSystemLocalesForTests({ ui: 'sv', format: 'sv-SE' })
    const onMoved = vi.fn()
    await watchSystemLocales(onMoved)

    onEvent?.({ locales: { ui: 'hu', format: 'sv-SE' } })

    expect(pickUiLocale('system')).toBe('hu')
    expect(onMoved).toHaveBeenCalledOnce()
  })

  it('adopts a moved region even though the language stayed', async () => {
    // Switching System Settings > Region changes no copy at all, and changes
    // every date and grouped number. The re-render comes from the caller, so
    // the tag has to be in place before `onMoved` runs.
    _setSystemLocalesForTests({ ui: 'en', format: 'en-US' })
    const seenWhileApplying = vi.fn()
    await watchSystemLocales(() => {
      seenWhileApplying(getFormatLocale())
    })

    onEvent?.({ locales: { ui: 'en', format: 'en-SE' } })

    expect(getFormatLocale()).toBe('en-SE')
    expect(seenWhileApplying).toHaveBeenCalledWith('en-SE')
  })

  it('stays silent on an answer the app already has', async () => {
    // Every call re-renders every open `t()` in the window, so a stray or
    // replayed event must cost nothing.
    _setSystemLocalesForTests({ ui: 'sv', format: 'sv-SE' })
    const onMoved = vi.fn()
    await watchSystemLocales(onMoved)

    onEvent?.({ locales: { ui: 'sv', format: 'sv-SE' } })

    expect(onMoved).not.toHaveBeenCalled()
  })
})
