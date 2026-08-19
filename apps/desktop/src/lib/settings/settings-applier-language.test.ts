/**
 * The settings-applier's language wiring: changing `appearance.language` must
 * live-apply by driving the i18n locale-switch seam (`setLocale`), with no Tauri
 * command and no restart, and the persisted choice must be applied at startup.
 *
 * We mock the i18n runtime (to observe `setLocale`) and the settings store (to
 * drive a controllable change pipeline + a controllable startup value), so the
 * test isolates the applier's mapping: `'system'` → the locale Rust resolved
 * from the OS preference list, a real tag → `setLocale(tag)`.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { _setSystemUiLocaleForTests } from '$lib/intl/ui-locale'

const setLocale = vi.fn()

// The change listener the applier registers, captured so the test can fire it.
let changeListener: ((id: string, value: unknown) => void) | undefined
// The startup value `getSetting('appearance.language')` returns.
let startupLanguage = 'system'

vi.mock('$lib/intl/messages.svelte', async (importOriginal) => {
  const actual = await importOriginal<typeof import('$lib/intl/messages.svelte')>()
  return {
    ...actual,
    // Spy on the locale-switch seam; keep `availableLocales` (the registry needs
    // it to build the language options) and everything else real. Braces keep the
    // arrow void (the spy's signature is `(tag) => void`, so don't return the call).
    setLocale: (locale: string | null) => {
      setLocale(locale)
    },
  }
})

// The applier fires fire-and-forget backend pushes at startup (file-watcher
// debounce, error reports, etc.) through the typed Tauri wrappers. In the test
// env those hit an unmocked `invoke`, so stub the module's functions to no-op
// promises: this test only cares about the language path, not the backend sync.
vi.mock('$lib/tauri-commands', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  const noop = () => Promise.resolve()
  const stubbed: Record<string, unknown> = {}
  for (const key of Object.keys(actual)) {
    stubbed[key] = typeof actual[key] === 'function' ? noop : actual[key]
  }
  return stubbed
})

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<typeof import('$lib/settings')>()
  return {
    ...actual,
    initializeSettings: vi.fn().mockResolvedValue(undefined),
    getSetting: (id: string) => (id === 'appearance.language' ? startupLanguage : actual.getSetting(id as never)),
    onSettingChange: (listener: (id: string, value: unknown) => void) => {
      changeListener = listener
      return () => {
        changeListener = undefined
      }
    },
  }
})

// The applier fires several fire-and-forget backend pushes during startup; the
// typed wrappers go through mocked Tauri IPC (see test-setup), so they resolve
// harmlessly. We only assert on the language path.
import { initSettingsApplier, cleanupSettingsApplier } from './settings-applier'

beforeEach(() => {
  setLocale.mockClear()
  changeListener = undefined
  startupLanguage = 'system'
  _setSystemUiLocaleForTests(null)
})

afterEach(() => {
  cleanupSettingsApplier()
})

describe('settings-applier: appearance.language', () => {
  it('applies a persisted real locale at startup (survives restart)', async () => {
    startupLanguage = 'en-XA'
    await initSettingsApplier()
    expect(setLocale).toHaveBeenCalledWith('en-XA')
  })

  it('applies the OS-resolved locale at startup for System default', async () => {
    // The point of the whole feature: `'system'` means the first language we
    // ship out of the user's ORDERED preference list, which only Rust can see.
    startupLanguage = 'system'
    _setSystemUiLocaleForTests('sv')
    await initSettingsApplier()
    expect(setLocale).toHaveBeenCalledWith('sv')
  })

  it('applies System default as a null override when the OS has no answer', async () => {
    // Off macOS, or when the read failed: the webview default is the honest
    // fallback, and the catalog resolver walks it down to `en`.
    startupLanguage = 'system'
    await initSettingsApplier()
    expect(setLocale).toHaveBeenCalledWith(null)
  })

  it('live-applies a switch to a real locale (no restart, no Tauri command)', async () => {
    await initSettingsApplier()
    setLocale.mockClear()
    expect(changeListener).toBeDefined()
    changeListener?.('appearance.language', 'en-XA')
    expect(setLocale).toHaveBeenCalledTimes(1)
    expect(setLocale).toHaveBeenCalledWith('en-XA')
  })

  it('live-applies a switch back to System default as the OS-resolved locale', async () => {
    _setSystemUiLocaleForTests('hu')
    await initSettingsApplier()
    setLocale.mockClear()
    changeListener?.('appearance.language', 'system')
    expect(setLocale).toHaveBeenCalledTimes(1)
    expect(setLocale).toHaveBeenCalledWith('hu')
  })
})
