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
 * `routes/reactive-settings-coverage.test.ts`.
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
