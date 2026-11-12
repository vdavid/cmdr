/**
 * Runtime tests for the i18n message layer: resolution, the fallback chain, and
 * the load-bearing read-rune-before-cache reactivity invariant.
 *
 * `.svelte.` infix: the reactivity test mounts a real component that reads `t()`
 * in markup and drives a locale change through `setLocale()` (which bumps the
 * version rune), proving `{t('key')}` re-renders. Driving via
 * `_setLocaleForTests` (value only, no rune bump) would NOT re-render: that's
 * the seam distinction Decision 6 warns about.
 */
import { describe, it, expect, afterEach, onTestFinished } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import { withPlatform } from '#lib/platform/test-platform.js'
import {
  t,
  tString,
  tStringIn,
  getMessage,
  pathParam,
  setLocale,
  availableLocales,
  resolvedCatalogLocale,
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
  void setLocale(null)
  document.documentElement.lang = 'en'
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
    // `screenshots/` sits alongside the locale dirs under `messages/`. The glob
    // excludes it outright, and `BCP47_DIR` would reject it anyway; this pins the
    // user-visible half, that it can't surface as a fake locale.
    expect(availableLocales()).not.toContain('screenshots')
  })

  it('gives `screenshots/` no loader, so its files never reach the bundle', async () => {
    // `screenshots/capture-report.json` is translator tooling that weighed ~280 kB, 4.5%
    // of the whole frontend, before it was kept out. A catalog chunk exists only for a
    // tag the manifest names, so the manifest is the gate (`vite-locale-catalogs.ts`).
    const { shippedLocales, loaders } = await import('virtual:cmdr-locale-manifest')
    expect(shippedLocales).not.toContain('screenshots')
    expect(Object.keys(loaders)).not.toContain('screenshots')
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

  // A same-named twin's row path carries a NUL pick (`#lib/utils/display-path`);
  // no sentence a person reads shows it.
  it('shows a twin row path in a param as its plain path', () => {
    _setLocaleForTests('en-US')
    expect(tString('fileOperations.delete.fromPath', { path: '/DCIM/\u00004242/IMG_1.JPG' })).toBe(
      'From: /DCIM/IMG_1.JPG',
    )
  })

  it('shows a path param with backslashes on Windows, and as is elsewhere', () => {
    _setLocaleForTests('en-US')
    onTestFinished(withPlatform('windows'))
    expect(tString('fileOperations.delete.fromPath', { path: pathParam('C:/Users/ada') })).toBe('From: C:\\Users\\ada')
    onTestFinished(withPlatform('macos'))
    expect(tString('fileOperations.delete.fromPath', { path: pathParam('C:/Users/ada') })).toBe('From: C:/Users/ada')
    onTestFinished(withPlatform('linux'))
    expect(tString('fileOperations.delete.fromPath', { path: pathParam('/home/ada') })).toBe('From: /home/ada')
  })

  /** #1118: only a param its caller marks as a path is spelled as one; prose stays prose. */
  it('leaves an unmarked string param alone on Windows, even one shaped like a path', () => {
    _setLocaleForTests('en-US')
    onTestFinished(withPlatform('windows'))
    expect(tString('main.revealNotShown.notAnswering', { name: 'C:' })).toBe(
      'Couldn’t show “C:”: the drive or server it’s on isn’t answering.',
    )
    expect(tString('main.revealNotShown.notAnswering', { name: '//a/b' })).toBe(
      'Couldn’t show “//a/b”: the drive or server it’s on isn’t answering.',
    )
  })

  it('spells a marked drive root the way Windows shows it', () => {
    _setLocaleForTests('en-US')
    onTestFinished(withPlatform('windows'))
    expect(tString('fileOperations.delete.fromPath', { path: pathParam('C:/') })).toBe('From: C:\\')
    expect(tString('fileOperations.delete.fromPath', { path: pathParam('//nas/share/x') })).toBe(
      'From: \\\\nas\\share\\x',
    )
  })
})

describe('the script boundary in the fallback chain', () => {
  // The rule the Rust resolver already enforces for auto-selection, now enforced
  // per key at runtime too: a fallback is only a kindness when it lands somewhere
  // the reader can READ. Simplified Chinese in front of a Traditional reader is
  // worse than English. Canonical rationale:
  // `apps/desktop/src-tauri/src/intl/DETAILS.md`
  // § The script guard, and why regional fallback survives it.
  // `zh` and `zh-Hant` are REAL shipped catalogs, and these tests stand fakes up
  // in their place. `_setCatalogForTests(tag, null)` RESTORES the shipped one, so
  // this teardown leaves the runtime as it found it and no later test in the file
  // depends on sitting above this block.
  afterEach(() => {
    for (const tag of ['zh', 'zh-Hant', 'zh-Hant-TW', 'zh-TW']) _setCatalogForTests(tag, null)
  })

  it('never resolves a Traditional reader through the Simplified catalog', () => {
    _setCatalogForTests('zh', { 'transfer.trash': 'SIMPLIFIED' })
    _setCatalogForTests('zh-Hant', {}) // a GAP in the Traditional catalog, so the chain has to walk
    _setLocaleForTests('zh-Hant')
    // `zh` is the language base, but it's Hans: English is the honest answer.
    expect(tString('transfer.trash', { countText: '2', count: 2 })).toBe('Moved 2 files to trash')
  })

  it('blocks the Simplified catalog for a region tag that implies Traditional', () => {
    _setCatalogForTests('zh', { 'transfer.trash': 'SIMPLIFIED' })
    _setLocaleForTests('zh-TW') // no script subtag; CLDR says TW is Hant
    expect(tString('transfer.trash', { countText: '2', count: 2 })).toBe('Moved 2 files to trash')
  })

  it('walks to a same-script ancestor instead of stopping at the language base', () => {
    _setCatalogForTests('zh', { 'transfer.trash': 'SIMPLIFIED' })
    _setCatalogForTests('zh-Hant', { 'transfer.trash': 'TRADITIONAL' })
    _setLocaleForTests('zh-Hant-TW') // → zh-Hant (Hant), never zh (Hans)
    expect(tString('transfer.trash', { countText: '2', count: 2 })).toBe('TRADITIONAL')
  })

  it('keeps regional fallback working, which is a papercut and not a wall', () => {
    _setCatalogForTests('zh', { 'transfer.trash': 'SIMPLIFIED' })
    _setLocaleForTests('zh-CN') // same script as `zh`
    expect(tString('transfer.trash', { countText: '2', count: 2 })).toBe('SIMPLIFIED')
  })
})

describe('an overlay whose base is not English', () => {
  // `es-419` is the shape a Latin American overlay takes over `es`, the way
  // `en-GB` overlays `en`: it forks a handful of keys and inherits the rest.
  // With `en` as the base the inherited text is English anyway, so only a
  // non-English base shows whether the chain really walks through it.
  afterEach(() => {
    for (const tag of ['es', 'es-419']) _setCatalogForTests(tag, null)
  })

  it('reads an unforked key from its base language, not English', () => {
    _setCatalogForTests('es', { 'transfer.trash': 'SPAIN', 'common.attachEmail': 'INHERITED' })
    _setCatalogForTests('es-419', { 'transfer.trash': 'LATAM' })
    _setLocaleForTests('es-419')
    expect(tString('transfer.trash', { countText: '1', count: 1 })).toBe('LATAM')
    expect(getMessage('common.attachEmail')).toBe('INHERITED')
    expect(resolvedCatalogLocale('es-419')).toBe('es-419')
  })

  it('leaves Spain on its own catalog', () => {
    _setCatalogForTests('es', { 'transfer.trash': 'SPAIN' })
    _setCatalogForTests('es-419', { 'transfer.trash': 'LATAM' })
    _setLocaleForTests('es')
    expect(tString('transfer.trash', { countText: '1', count: 1 })).toBe('SPAIN')
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
    // A per-OS value: the raw reader hands back this OS's branch, tags and all.
    onTestFinished(withPlatform('macos'))
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

describe('platform variants', () => {
  const ICU_BLOCK =
    // allowed-pluralize-noun: `} windows` is a platform block's branch keyword, not a count and a noun
    '{platform, select, macos {Moved {count, plural, one {# file} other {# files}} to the Trash} windows {Moved {count, plural, one {# file} other {# files}} to the Recycle Bin} other {Moved {count, plural, one {# file} other {# files}} to the trash}}'
  const RAW_BLOCK =
    '{platform, select, macos {Open {system_settings} and unlock it.} other {Run `chmod u+w` on it, e.g. via a terminal.}}'

  function seed(): void {
    _setCatalogForTests(TEST_LOCALE, {
      'transfer.trash': ICU_BLOCK,
      'errors.write.fileLocked.message': RAW_BLOCK,
    })
    _setLocaleForTests(TEST_LOCALE)
  }

  it('t() renders the branch of the platform it runs on, with no cache reset between flips', () => {
    seed()
    onTestFinished(withPlatform('macos'))
    expect(tString('transfer.trash', { count: 2 })).toBe('Moved 2 files to the Trash')
    withPlatform('windows')
    expect(tString('transfer.trash', { count: 1 })).toBe('Moved 1 file to the Recycle Bin')
    withPlatform('linux')
    expect(tString('transfer.trash', { count: 3 })).toBe('Moved 3 files to the trash')
    withPlatform('macos')
    expect(tString('transfer.trash', { count: 1 })).toBe('Moved 1 file to the Trash')
  })

  it('t() with no params still selects', () => {
    _setCatalogForTests(TEST_LOCALE, { 'transfer.trash': '{platform, select, macos {Finder} other {Files}}' })
    _setLocaleForTests(TEST_LOCALE)
    onTestFinished(withPlatform('macos'))
    expect(t('transfer.trash')).toBe('Finder')
    withPlatform('windows')
    expect(t('transfer.trash')).toBe('Files')
  })

  it('getMessage() returns the raw branch text, tokens untouched', () => {
    seed()
    onTestFinished(withPlatform('macos'))
    expect(getMessage('errors.write.fileLocked.message')).toBe('Open {system_settings} and unlock it.')
    withPlatform('windows')
    expect(getMessage('errors.write.fileLocked.message')).toBe('Run `chmod u+w` on it, e.g. via a terminal.')
    withPlatform('linux')
    expect(getMessage('errors.write.fileLocked.message')).toBe('Run `chmod u+w` on it, e.g. via a terminal.')
    withPlatform('macos')
    expect(getMessage('errors.write.fileLocked.message')).toBe('Open {system_settings} and unlock it.')
  })

  it('tStringIn() picks the branch too', () => {
    _setCatalogForTests('en', { 'transfer.trash': ICU_BLOCK })
    onTestFinished(() => {
      _setCatalogForTests('en', null)
    })
    onTestFinished(withPlatform('windows'))
    expect(tStringIn('en', 'transfer.trash', { count: 2 })).toBe('Moved 2 files to the Recycle Bin')
    withPlatform('macos')
    expect(tStringIn('en', 'transfer.trash', { count: 2 })).toBe('Moved 2 files to the Trash')
  })

  it('a caller can’t pass `platform` itself', () => {
    seed()
    onTestFinished(withPlatform('macos'))
    // @ts-expect-error `platform` is the runtime's to inject
    const forced = tString('transfer.trash', { count: 1, platform: 'windows' })
    // And even an untyped caller loses: the runtime's answer wins.
    expect(forced).toBe('Moved 1 file to the Trash')
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

    // Drive via setLocale (bumps the rune): the reactive path.
    void setLocale(TEST_LANG)
    flushSync()
    expect(span?.textContent).toBe('SWITCHED 1')

    void unmount(component)
  })

  it('re-renders a markup number on a language switch, with no message involved', () => {
    // Numbers follow the app language (`locale.ts::getCatalogLocale`), so a switch must reach
    // a `{formatInteger(n)}` in markup the way it reaches a `{t()}`.
    void setLocale('en')
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(Fixture, { target, props: { messageKey: 'transfer.trash' } })
    flushSync()
    const span = target.querySelector('[data-test="number-text"]')
    expect(span?.textContent).toBe('1,234,567')

    void setLocale('de')
    flushSync()
    expect(span?.textContent).toBe('1.234.567')

    void unmount(component)
    void setLocale(null)
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

describe('the catalog actually supplying the text', () => {
  it('resolves a shipped tag to itself', () => {
    expect(resolvedCatalogLocale('hu')).toBe('hu')
  })

  it('keeps a script variant rather than collapsing it to the base language', () => {
    // `zh-Hant` must NOT resolve to Simplified `zh`: the fallback chain refuses
    // to cross a script boundary, so the text on screen really is Traditional.
    expect(resolvedCatalogLocale('zh-Hant')).toBe('zh-Hant')
  })

  it('resolves an overlay tag to the overlay, not to its base', () => {
    expect(resolvedCatalogLocale('en-GB')).toBe('en-GB')
  })

  it('falls back to the regional parent we do ship', () => {
    // We ship `pt`, not `pt-PT`, so `pt` is what a European Portuguese speaker reads.
    expect(resolvedCatalogLocale('pt-PT')).toBe('pt')
  })

  it('falls back to English for a language we ship nothing for', () => {
    expect(resolvedCatalogLocale('ja-JP')).toBe('en')
  })
})

describe('the document language attribute', () => {
  it('follows a language switch', () => {
    void setLocale('zh-Hant')
    expect(document.documentElement.lang).toBe('zh-Hant')

    void setLocale('hu')
    expect(document.documentElement.lang).toBe('hu')
  })

  it('announces the catalog on screen, not the language that was asked for', () => {
    // A Japanese Mac with no Japanese catalog reads English text. Announcing
    // `ja-JP` would hand a screen reader a Japanese voice for English words.
    void setLocale('ja-JP')
    expect(document.documentElement.lang).toBe('en')
  })

  it('re-resolves when the override is dropped rather than going stale', () => {
    void setLocale('hu')
    void setLocale(null)
    expect(document.documentElement.lang).not.toBe('hu')
  })
})
