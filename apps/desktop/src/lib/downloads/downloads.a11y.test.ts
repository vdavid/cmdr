/**
 * Tier 3 a11y tests for the downloads surfaces: the download toast, the global
 * shortcut row and its animation, the shortcut warn toast, and the two
 * latest-download toasts.
 *
 * One file per component would cost about six times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own props and assertions.
 *
 * Two stubs disagree between blocks and are therefore mutable, installed by each
 * `describe` in its own `beforeEach`: `$lib/settings`' `getSetting` (the row wants
 * `true`, the warn toast wants the binding string) and the bindings command
 * `setGlobalGoToLatestShortcut` (the row's returns nothing, the warn toast's
 * resolves an ok result). Every `$lib/*` stub spreads the real module first, so a
 * block that never stubbed one still sees its un-stubbed exports: the shortcut row
 * and the warn toast both call the REAL `$lib/tauri-commands` wrapper, which lands
 * on the stubbed `commands` below exactly as it does un-merged.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const { getGlobalGoToLatestBindingMock } = vi.hoisted(() => ({
  getGlobalGoToLatestBindingMock: vi.fn<() => string>(),
}))

// What `getSetting` answers, and what the bindings command returns. Each block
// installs its own in `beforeEach`, so neither inherits the other's.
let settingValue: unknown = undefined
let setGlobalShortcutResult: unknown = undefined

vi.mock('./go-to-latest', () => ({
  goToDownload: vi.fn(() => Promise.resolve()),
}))

vi.mock('./notifications-mode', () => ({
  setDownloadsNotificationsMode: vi.fn(),
  openSettingsToDownloadsNotifications: vi.fn(() => Promise.resolve()),
}))

vi.mock('./downloads-toast-collapsed', () => ({
  setDownloadsToastCollapsed: vi.fn(),
}))

vi.mock('./global-shortcut-setting', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getGlobalGoToLatestBinding: getGlobalGoToLatestBindingMock,
  setGlobalGoToLatestBinding: vi.fn(),
  GLOBAL_GO_TO_LATEST_BINDING_KEY: 'behavior.fileSystemWatching.globalGoToLatestShortcut.binding',
  GLOBAL_GO_TO_LATEST_ENABLED_KEY: 'behavior.fileSystemWatching.globalGoToLatestShortcut.enabled',
}))

vi.mock('$lib/ui/toast', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dismissToast: vi.fn(),
}))

vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getSetting: vi.fn(() => settingValue),
  setSetting: vi.fn(),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/ipc/bindings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  commands: { setGlobalGoToLatestShortcut: vi.fn(() => setGlobalShortcutResult) },
}))

vi.mock('$lib/logging/logger', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getAppLogger: () => ({ warn: vi.fn(), debug: vi.fn(), info: vi.fn(), error: vi.fn() }),
}))

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openPrivacySettings: vi.fn(() => Promise.resolve()),
}))

import DownloadToastContent from './DownloadToastContent.svelte'
import GlobalShortcutAnimation from './GlobalShortcutAnimation.svelte'
import GlobalShortcutRow from './GlobalShortcutRow.svelte'
import GlobalShortcutWarnToastContent from './GlobalShortcutWarnToastContent.svelte'
import LatestDownloadEmptyToastContent from './LatestDownloadEmptyToastContent.svelte'
import LatestDownloadFdaToastContent from './LatestDownloadFdaToastContent.svelte'

// These components share one jsdom document, and axe resolves ARIA id references
// document-wide. Clearing between tests keeps each audit looking at its own
// container only.
afterEach(() => {
  document.body.innerHTML = ''
})

describe('DownloadToastContent a11y', () => {
  const baseEvent = {
    path: '/Users/me/Downloads/report.pdf',
    parentDir: '/Users/me/Downloads',
    fileName: 'report.pdf',
    observedAtMs: 1_700_000_000_000,
    inSubdir: false,
    sizeBytes: 1024,
  }

  it('renders the expanded state with no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DownloadToastContent, {
      target,
      props: {
        toastId: 'downloads:a11y',
        explorer: undefined,
        event: baseEvent,
        shortcutHint: '⌘J',
        globalBinding: '⌃⌥⌘J',
        initialCollapsed: false,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders the collapsed state with no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DownloadToastContent, {
      target,
      props: {
        toastId: 'downloads:a11y-collapsed',
        explorer: undefined,
        event: baseEvent,
        shortcutHint: '⌘J',
        globalBinding: '⌃⌥⌘J',
        initialCollapsed: true,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

describe('GlobalShortcutAnimation a11y', () => {
  it('is a decorative, aria-hidden SVG with no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(GlobalShortcutAnimation, { target })
    await tick()

    // Decorative only: the toast states the keys in text, so the SVG is hidden
    // from assistive tech and out of the tab order.
    const svg = target.querySelector('svg')
    expect(svg?.getAttribute('aria-hidden')).toBe('true')
    expect(svg?.getAttribute('focusable')).toBe('false')

    await expectNoA11yViolations(target)
  })
})

describe('GlobalShortcutRow a11y', () => {
  beforeEach(() => {
    getGlobalGoToLatestBindingMock.mockReset()
    settingValue = true
    setGlobalShortcutResult = undefined
  })

  it('renders the default (unmodified) state with no a11y violations', async () => {
    getGlobalGoToLatestBindingMock.mockReturnValue('⌃⌥⌘J')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(GlobalShortcutRow, { target })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders the modified state (with reset button) with no a11y violations', async () => {
    getGlobalGoToLatestBindingMock.mockReturnValue('⌃⌥⌘K')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(GlobalShortcutRow, { target })
    await tick()
    await expectNoA11yViolations(target)
  })
})

describe('GlobalShortcutWarnToastContent a11y', () => {
  beforeEach(() => {
    settingValue = '⌃⌥⌘J'
    setGlobalShortcutResult = Promise.resolve({ status: 'ok', data: null })
  })

  it('renders with no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(GlobalShortcutWarnToastContent, {
      target,
      props: { toastId: 'shortcut-warn', binding: '⌃⌥⌘J' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

describe('LatestDownloadEmptyToastContent a11y', () => {
  it('renders with no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LatestDownloadEmptyToastContent, {
      target,
      props: { toastId: 'test-empty-toast', onGoToDownloads: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

describe('LatestDownloadFdaToastContent a11y', () => {
  it('renders with no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LatestDownloadFdaToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })
})
