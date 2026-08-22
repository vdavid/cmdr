/**
 * Pins the route → settings-access classification the root layout's
 * `initWindowSettings()` reads.
 *
 * Two ways this drifts silently: a new window route lands with no entry (and
 * gets the `'full'` fallback, which throws in a window with no store grant), or
 * a capability file drops `store:default` without the map following (and the
 * window loads settings through a plugin it isn't allowed to call). Both are
 * checked here against `src-tauri/capabilities/*.json`, the actual grant.
 *
 * The companion guard that every window initializes at all is
 * `routes/window-route-coverage.test.ts`.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { readFileSync, readdirSync, existsSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { WINDOW_SETTINGS_ACCESS, windowSettingsAccess } from './window-settings'

const here = path.dirname(fileURLToPath(import.meta.url))
// here = apps/desktop/src/lib/settings → the desktop app root is three up.
const desktopRoot = path.resolve(here, '../../..')
const routesRoot = path.join(desktopRoot, 'src/routes')
const capabilitiesRoot = path.join(desktopRoot, 'src-tauri/capabilities')

/** Route paths for every `+page.svelte`, with SvelteKit group segments `(x)` dropped. */
function routePaths(): string[] {
  const found: string[] = []
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name)
      if (entry.isDirectory()) walk(full)
      else if (entry.name === '+page.svelte') {
        const segments = path
          .relative(routesRoot, path.dirname(full))
          .split(path.sep)
          .filter((segment) => segment !== '' && segment !== '.' && !segment.startsWith('('))
        found.push(`/${segments.join('/')}`)
      }
    }
  }
  walk(routesRoot)
  return [...new Set(found)].sort()
}

describe('windowSettingsAccess', () => {
  it('classifies every window route', () => {
    expect(routePaths()).toEqual(Object.keys(WINDOW_SETTINGS_ACCESS).sort())
  })

  it('agrees with the capability files about which windows can reach the store', () => {
    // Route path → capability file. The main window's route is `/`.
    const capabilityForRoute: Record<string, string> = {
      '/': 'default.json',
      '/debug': 'debug.json',
      '/queue': 'queue.json',
      '/settings': 'settings.json',
      '/shortcuts': 'shortcuts.json',
      '/viewer': 'viewer.json',
    }

    for (const [route, file] of Object.entries(capabilityForRoute)) {
      const full = path.join(capabilitiesRoot, file)
      expect(existsSync(full), `${file} is missing`).toBe(true)
      const capability = JSON.parse(readFileSync(full, 'utf8')) as { permissions?: unknown[] }
      const grantsStore = (capability.permissions ?? []).some(
        (permission) => typeof permission === 'string' && permission.startsWith('store:'),
      )
      expect(windowSettingsAccess(route), `${route} (${file})`).toBe(grantsStore ? 'full' : 'restricted')
    }
  })

  it('tolerates the static adapter trailing slash', () => {
    expect(windowSettingsAccess('/queue/')).toBe('restricted')
    expect(windowSettingsAccess('/settings/')).toBe('full')
  })

  it('falls back to full access for an unmapped path', () => {
    // A capability grants the store unless someone deliberately dropped it, so a
    // wrong guess here degrades to a load error rather than a silently-default window.
    expect(windowSettingsAccess('/not-a-window')).toBe('full')
  })
})

describe('initWindowSettings', () => {
  beforeEach(() => {
    vi.resetModules()
  })

  afterEach(() => {
    vi.doUnmock('./reactive-settings.svelte')
  })

  it('takes the restricted path for a window with no store grant', async () => {
    const initReactiveSettings = vi.fn().mockResolvedValue(undefined)
    vi.doMock('./reactive-settings.svelte', () => ({ initReactiveSettings }))

    const { initWindowSettings } = await import('./window-settings')
    await initWindowSettings('/queue')

    expect(initReactiveSettings).toHaveBeenCalledWith({ restrictedWindow: true })
  })

  it('takes the full path for a window that can reach the store', async () => {
    const initReactiveSettings = vi.fn().mockResolvedValue(undefined)
    vi.doMock('./reactive-settings.svelte', () => ({ initReactiveSettings }))

    const { initWindowSettings } = await import('./window-settings')
    await initWindowSettings('/settings')

    expect(initReactiveSettings).toHaveBeenCalledWith({ restrictedWindow: false })
  })
})

describe('initWindowLanguageSync', () => {
  /** The change listener the sync registers, so a test can fire it. */
  let changeListener: ((id: string, value: string) => void) | undefined
  /** The OS-language-moved callback the sync registers, so a test can fire it. */
  let osLocaleMoved: (() => void) | undefined
  let language = 'system'
  const unsubscribe = vi.fn()
  const unlistenOsLocale = vi.fn()
  const setLocale = vi.fn()

  beforeEach(() => {
    vi.resetModules()
    changeListener = undefined
    osLocaleMoved = undefined
    unlistenOsLocale.mockClear()
    language = 'system'
    unsubscribe.mockClear()
    setLocale.mockClear()
    vi.doMock('./settings-store', () => ({
      getSetting: () => language,
      onSpecificSettingChange: (_id: string, listener: (id: string, value: string) => void) => {
        changeListener = listener
        return unsubscribe
      },
    }))
    // Partial: the settings registry (pulled in transitively) calls
    // `availableLocales()` to build the Language picker's options.
    vi.doMock('$lib/intl/messages.svelte', async (importOriginal) => ({
      ...(await importOriginal<typeof import('$lib/intl/messages.svelte')>()),
      setLocale,
    }))
  })

  afterEach(() => {
    vi.doUnmock('./settings-store')
    vi.doUnmock('$lib/intl/messages.svelte')
    vi.doUnmock('$lib/intl/os-locales')
  })

  /** Loads the module with a controllable OS answer behind `loadSystemLocales`. */
  async function loadWith(systemLocale: string | null) {
    vi.doMock('$lib/intl/os-locales', () => ({
      loadSystemLocales: () => Promise.resolve({ ui: systemLocale, format: null }),
      pickUiLocale: (setting: string) => (setting === 'system' ? systemLocale : setting),
      watchSystemLocales: (onMoved: () => void) => {
        osLocaleMoved = onMoved
        return Promise.resolve(unlistenOsLocale)
      },
    }))
    return import('./window-settings')
  }

  it('applies an explicit language without waiting for the OS answer', async () => {
    language = 'hu'
    const { initWindowLanguageSync } = await loadWith('sv')
    initWindowLanguageSync()
    // Synchronously, before any await: a secondary window must not gate its
    // first paint on an IPC round-trip.
    expect(setLocale).toHaveBeenCalledWith('hu')
  })

  it('re-applies once the OS answer lands, so `system` stops being the webview default', async () => {
    const { initWindowLanguageSync } = await loadWith('sv')
    initWindowLanguageSync()
    await vi.waitFor(() => {
      expect(setLocale).toHaveBeenCalledWith('sv')
    })
  })

  it('follows a live language change', async () => {
    const { initWindowLanguageSync } = await loadWith(null)
    initWindowLanguageSync()
    setLocale.mockClear()
    changeListener?.('appearance.language', 'de')
    expect(setLocale).toHaveBeenCalledWith('de')
  })

  it('follows the OS language moving underneath the window', async () => {
    // `'system'` means the language the user reads NOW: a switch in System
    // Settings has to re-localize this window without a restart.
    const { initWindowLanguageSync } = await loadWith('de')
    initWindowLanguageSync()
    await vi.waitFor(() => {
      expect(osLocaleMoved).toBeDefined()
    })
    setLocale.mockClear()
    osLocaleMoved?.()
    expect(setLocale).toHaveBeenCalledWith('de')
  })

  it('stops following both sources when the window closes', async () => {
    const { initWindowLanguageSync } = await loadWith(null)
    const stop = initWindowLanguageSync()
    await vi.waitFor(() => {
      expect(osLocaleMoved).toBeDefined()
    })
    stop()
    expect(unsubscribe).toHaveBeenCalled()
    expect(unlistenOsLocale).toHaveBeenCalled()
  })
})
