/**
 * Runtime tests for the i18n message layer: resolution, the fallback chain, and
 * the load-bearing read-rune-before-cache reactivity invariant.
 *
 * `.svelte.` infix: the reactivity test mounts a real component that reads `t()`
 * in markup and drives a locale change through `setLocale()` (which bumps the
 * version rune), proving `{t('key')}` re-renders. Driving via
 * `_setLocaleForTests` (value only, no rune bump) would NOT re-render — that's
 * the seam distinction Decision 6 warns about.
 */
import { describe, it, expect, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import {
  t,
  tString,
  getMessage,
  setLocale,
  availableLocales,
  _setCatalogForTests,
  _clearCompiledCacheForTests,
  _resetCaptureForTests,
} from './messages.svelte'
import { _setLocaleForTests } from './locale'
import Fixture from './messages-reactivity-fixture.svelte'

interface I18nCaptureApi {
  enable: () => boolean
  disable: () => void
  setSurface: (label: string) => void
  dump: () => Record<string, string[]>
  reset: () => void
}
function captureApi(): I18nCaptureApi {
  const api = (window as unknown as { __cmdrI18nCapture?: I18nCaptureApi }).__cmdrI18nCapture
  if (api === undefined) throw new Error('__cmdrI18nCapture not installed (expected outside prod)')
  return api
}

const TEST_LOCALE = 'zz-ZZ'
const TEST_LANG = 'zz'

afterEach(() => {
  setLocale(null)
  _setLocaleForTests(null)
  _setCatalogForTests(TEST_LOCALE, null)
  _setCatalogForTests(TEST_LANG, null)
  _clearCompiledCacheForTests()
  _resetCaptureForTests()
})

describe('capture mode (screenshot-coupling instrumentation)', () => {
  it('records nothing while disabled (zero-cost default)', () => {
    _setLocaleForTests('en-US')
    const api = captureApi()
    api.reset()
    tString('transfer.trash', { countText: '1', count: 1 })
    getMessage('common.downloadsFdaHint')
    expect(api.dump()).toEqual({})
  })

  it('records resolved keys against the active surface for both t() and getMessage()', () => {
    _setLocaleForTests('en-US')
    const api = captureApi()
    api.reset()
    expect(api.enable()).toBe(true)
    api.setSurface('main-window')
    tString('transfer.trash', { countText: '1', count: 1 })
    getMessage('common.downloadsFdaHint')
    api.setSurface('a-dialog')
    tString('transfer.split.clean', { verb: 'copy', phrase: '2 files' })
    api.disable()

    expect(api.dump()).toEqual({
      'main-window': ['common.downloadsFdaHint', 'transfer.trash'],
      'a-dialog': ['transfer.split.clean'],
    })
  })

  it('stops recording after disable()', () => {
    _setLocaleForTests('en-US')
    const api = captureApi()
    api.reset()
    api.enable()
    api.setSurface('s1')
    tString('transfer.trash', { countText: '1', count: 1 })
    api.disable()
    tString('transfer.split.clean', { verb: 'copy', phrase: '2 files' })
    expect(api.dump()).toEqual({ s1: ['transfer.trash'] })
  })
})

describe('availableLocales() (loaded-catalog discovery + non-locale-dir exclusion)', () => {
  it('includes the base `en` catalog and lists it first', () => {
    const locales = availableLocales()
    expect(locales).toContain('en')
    expect(locales[0]).toBe('en')
  })

  it('never treats the `screenshots/` capture-artifact dir as a locale', () => {
    // `screenshots/` sits alongside the locale dirs under `messages/` and is
    // globbed by `messages/*/*.json`, but it's not a BCP-47 tag, so the runtime
    // must filter it out. A regression here would surface it as a fake locale.
    expect(availableLocales()).not.toContain('screenshots')
  })

  it('only lists BCP-47-shaped tags', () => {
    const bcp47 = /^[a-z]{2,3}(-[a-z0-9]+)*$/i
    for (const tag of availableLocales()) {
      expect(tag).toMatch(bcp47)
    }
  })
})

describe('t() resolution', () => {
  it('resolves and ICU-formats a plural message at the active locale', () => {
    _setLocaleForTests('en-US')
    expect(tString('transfer.trash', { countText: '3', count: 3 })).toBe('Moved 3 files to trash')
    expect(tString('transfer.trash', { countText: '1', count: 1 })).toBe('Moved 1 file to trash')
  })

  it('runs trivial interpolation through the engine (one code path)', () => {
    _setLocaleForTests('en-US')
    expect(tString('transfer.split.clean', { verb: 'copy', phrase: '2 files' })).toBe('Copied 2 files.')
  })
})

describe('fallback chain (locale → base language → en → key)', () => {
  it('prefers an exact-locale catalog entry when present', () => {
    _setCatalogForTests(TEST_LOCALE, { 'transfer.trash': 'EXACT' })
    _setLocaleForTests(TEST_LOCALE)
    expect(tString('transfer.trash', { countText: '1', count: 1 })).toBe('EXACT')
  })

  it('falls back from a region tag to its base-language catalog', () => {
    _setCatalogForTests(TEST_LANG, { 'transfer.trash': 'LANG' })
    _setLocaleForTests(TEST_LOCALE) // zz-ZZ → zz
    expect(tString('transfer.trash', { countText: '1', count: 1 })).toBe('LANG')
  })

  it('falls back to en when neither the locale nor its base language has the key', () => {
    _setLocaleForTests(TEST_LOCALE)
    expect(tString('transfer.trash', { countText: '2', count: 2 })).toBe('Moved 2 files to trash')
  })

  it('falls back to the key string when the key is missing everywhere (never crashes)', () => {
    _setLocaleForTests('en-US')
    // @ts-expect-error deliberately-unknown key to exercise the last fallback
    expect(t('transfer.doesNotExist')).toBe('transfer.doesNotExist')
  })
})

describe('getMessage() raw accessor', () => {
  it('returns the raw catalog string without ICU parsing', () => {
    _setLocaleForTests('en-US')
    expect(getMessage('common.downloadsFdaHint')).toBe(
      'Cmdr needs Full Disk Access to watch your Downloads folder. <settingsLink>Open System Settings</settingsLink>',
    )
  })

  it('uses the same fallback chain as t()', () => {
    _setCatalogForTests(TEST_LOCALE, { 'transfer.trash': 'RAW EXACT' })
    _setLocaleForTests(TEST_LOCALE)
    expect(getMessage('transfer.trash')).toBe('RAW EXACT')
  })
})

describe('reactivity in markup (read-rune-before-cache invariant)', () => {
  it('re-renders a markup t() usage when setLocale() bumps the version rune', () => {
    // A test-only second locale so the rendered text actually CHANGES on switch
    // (with only `en`, identical output couldn't distinguish a re-render).
    _setCatalogForTests(TEST_LANG, { 'transfer.trash': 'SWITCHED {countText}' })

    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(Fixture, { target, props: { messageKey: 'transfer.trash' } })
    flushSync()

    const span = target.querySelector('[data-test="trans-text"]')
    expect(span?.textContent).toBe('Moved 1 file to trash')

    // Drive via setLocale (bumps the rune) — the reactive path.
    setLocale(TEST_LANG)
    flushSync()
    expect(span?.textContent).toBe('SWITCHED 1')

    void unmount(component)
  })

  it('does NOT re-render when only the value changes without a rune bump (seam distinction)', () => {
    _setCatalogForTests(TEST_LANG, { 'transfer.trash': 'SWITCHED {countText}' })

    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(Fixture, { target, props: { messageKey: 'transfer.trash' } })
    flushSync()
    const span = target.querySelector('[data-test="trans-text"]')
    expect(span?.textContent).toBe('Moved 1 file to trash')

    // Value-only change: no rune bump, so markup must NOT re-render.
    _setLocaleForTests(TEST_LANG)
    _clearCompiledCacheForTests()
    flushSync()
    expect(span?.textContent).toBe('Moved 1 file to trash')

    void unmount(component)
  })
})
