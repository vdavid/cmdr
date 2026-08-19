/**
 * The two language events: one per launch, one per deliberate pick.
 *
 * The "once" halves are the parts worth pinning. `language_resolved` answers
 * "which language did this install come up in", so a second send would double a
 * launch; `language_changed` is the quality signal ("somebody walked away from
 * their own language"), so a send on the keyboard/hover preview, or on a pick
 * that lands where the user already was, would invent walk-aways that never
 * happened.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const trackEvent = vi.fn<(name: string, props: Record<string, unknown>) => void>()
vi.mock('$lib/tauri-commands', () => ({
  trackEvent: (name: string, props: Record<string, unknown>) => {
    trackEvent(name, props)
  },
  // `os-locales.ts` reaches for these at import time; the test drives it through
  // `_setSystemLocalesForTests` instead, so they only have to exist.
  getOsLocales: () => Promise.resolve({ ui: null, format: null }),
  onOsLocalesChanged: () => Promise.resolve(() => {}),
}))

let languageSetting = 'system'
vi.mock('$lib/settings', () => ({
  getSetting: () => languageSetting,
}))

import { _setLocaleForTests } from './locale'
import { _setSystemLocalesForTests } from './os-locales'
import {
  noteStartupLanguage,
  trackLanguageChanged,
  trackLanguageResolved,
  _resetLanguageAnalyticsForTests,
} from './language-analytics'

/** Puts the window in `tag`, the way an apply would, without a re-render. */
function running(tag: string): void {
  _setLocaleForTests(tag)
}

/** What the Rust resolver found in the OS preference list (`null` = nothing shipped matches). */
function detected(tag: string | null): void {
  _setSystemLocalesForTests({ ui: tag, format: null })
}

beforeEach(() => {
  trackEvent.mockClear()
  _resetLanguageAnalyticsForTests()
  languageSetting = 'system'
  detected(null)
  running('en-US')
})

describe('trackLanguageResolved', () => {
  it('reports an auto-selected language as the base subtag alone', () => {
    languageSetting = 'system'
    detected('pt-BR')
    running('pt-BR')

    trackLanguageResolved()

    expect(trackEvent).toHaveBeenCalledTimes(1)
    expect(trackEvent.mock.calls[0][0]).toBe('language_resolved')
    expect(trackEvent.mock.calls[0][1]).toEqual({ detected: 'pt', active: 'pt', source: 'auto' })
  })

  it('reports an explicit pick as explicit, and still says what the OS offered', () => {
    languageSetting = 'de'
    detected('hu-HU')
    running('de')

    trackLanguageResolved()

    expect(trackEvent.mock.calls[0][1]).toEqual({ detected: 'hu', active: 'de', source: 'explicit' })
  })

  it('reports no OS match as a fallback with no detected language', () => {
    languageSetting = 'system'
    detected(null)
    running('en-US')

    trackLanguageResolved()

    expect(trackEvent.mock.calls[0][1]).toEqual({ detected: 'none', active: 'en', source: 'fallback' })
  })

  it('sends once per launch, however often it is called', () => {
    trackLanguageResolved()
    trackLanguageResolved()
    trackLanguageResolved()

    expect(trackEvent).toHaveBeenCalledTimes(1)
  })
})

describe('trackLanguageChanged', () => {
  it('reports the language the user left, and where they left it from', () => {
    detected('hu-HU')
    running('hu')
    trackLanguageResolved()
    trackEvent.mockClear()

    trackLanguageChanged('onboarding', 'en')

    expect(trackEvent).toHaveBeenCalledTimes(1)
    expect(trackEvent.mock.calls[0][0]).toBe('language_changed')
    expect(trackEvent.mock.calls[0][1]).toEqual({ from: 'hu', surface: 'onboarding' })
  })

  it('says nothing when the pick lands on the language already running', () => {
    detected('hu-HU')
    running('hu')
    trackLanguageResolved()
    trackEvent.mockClear()

    // Pinning "System default (Magyar)" to an explicit `hu` is not a walk-away.
    trackLanguageChanged('settings', 'hu')

    expect(trackEvent).not.toHaveBeenCalled()
  })

  it('resolves a pick of `system` against what the OS offers', () => {
    languageSetting = 'de'
    detected('hu-HU')
    running('de')
    trackLanguageResolved()
    trackEvent.mockClear()

    trackLanguageChanged('settings', 'system')

    expect(trackEvent.mock.calls[0][1]).toEqual({ from: 'de', surface: 'settings' })
  })

  it('carries the previous language forward across consecutive picks', () => {
    detected('hu-HU')
    running('hu')
    trackLanguageResolved()
    trackEvent.mockClear()

    trackLanguageChanged('settings', 'de')
    trackLanguageChanged('settings', 'en')

    expect(trackEvent).toHaveBeenCalledTimes(2)
    expect(trackEvent.mock.calls[0][1]).toEqual({ from: 'hu', surface: 'settings' })
    expect(trackEvent.mock.calls[1][1]).toEqual({ from: 'de', surface: 'settings' })
  })

  it('reports the language a secondary window came up in, which never sends `language_resolved`', () => {
    // The Settings window hosts the settings picker but runs no startup event.
    running('sv')
    noteStartupLanguage()

    trackLanguageChanged('settings', 'en')

    expect(trackEvent).toHaveBeenCalledTimes(1)
    expect(trackEvent.mock.calls[0][1]).toEqual({ from: 'sv', surface: 'settings' })
  })

  it('leaves an already-known language alone when a startup seed lands late', () => {
    running('hu')
    noteStartupLanguage()
    trackLanguageChanged('settings', 'de')
    trackEvent.mockClear()

    noteStartupLanguage()
    trackLanguageChanged('settings', 'en')

    expect(trackEvent.mock.calls[0][1]).toEqual({ from: 'de', surface: 'settings' })
  })
})
