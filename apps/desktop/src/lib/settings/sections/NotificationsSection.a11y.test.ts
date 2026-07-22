/**
 * Tier-3 a11y tests for `NotificationsSection.svelte`.
 *
 * Covered states: default (FDA granted), FDA pending. Functional behavior
 * (card structure, anchor id, ToggleGroup write-through, IPC fire) is pinned in
 * the companion `.svelte.test.ts` file.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const { getSettingMock, setSettingMock, downloadsWatcherStatusMock, recheckGateMock, setGlobalGoToLatestShortcutMock } =
  vi.hoisted(() => ({
    getSettingMock: vi.fn(),
    setSettingMock: vi.fn(),
    downloadsWatcherStatusMock: vi.fn(),
    recheckGateMock: vi.fn(),
    setGlobalGoToLatestShortcutMock: vi.fn(),
  }))

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: getSettingMock,
  setSetting: setSettingMock,
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    downloadsWatcherStatus: downloadsWatcherStatusMock,
    recheckDownloadsWatcherGate: recheckGateMock,
    setGlobalGoToLatestShortcut: setGlobalGoToLatestShortcutMock,
  },
}))

import NotificationsSection from './NotificationsSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function setDefaultSettings(): void {
  getSettingMock.mockImplementation((key: string): unknown => {
    switch (key) {
      case 'behavior.fileSystemWatching.downloadsNotifications':
        return 'in-app'
      case 'behavior.fileSystemWatching.globalGoToLatestShortcut.enabled':
        return true
      case 'behavior.fileSystemWatching.globalGoToLatestShortcut.binding':
        return '\u{2303}\u{2325}\u{2318}J'
      case 'behavior.fileSystemWatching.globalGoToLatestShortcut.acknowledged':
        return true
      default:
        return undefined
    }
  })
}

function setStatus(fdaPending: boolean): void {
  downloadsWatcherStatusMock.mockResolvedValue({
    status: 'ok',
    data: { running: !fdaPending, downloadsDir: '/Users/me/Downloads', fdaPending },
  })
}

beforeEach(() => {
  getSettingMock.mockReset()
  setSettingMock.mockReset()
  downloadsWatcherStatusMock.mockReset()
  recheckGateMock.mockReset().mockResolvedValue({ status: 'ok', data: null })
  setGlobalGoToLatestShortcutMock.mockReset().mockResolvedValue({
    status: 'ok',
    data: { status: 'registered', binding: '\u{2303}\u{2325}\u{2318}J', enabled: true },
  })

  setDefaultSettings()
  setStatus(false)
})

async function mountSection(): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(NotificationsSection, { target, props: { searchQuery: '' } })
  // Drain the onMount IPC chain; jsdom needs a few flushes.
  await tick()
  await Promise.resolve()
  await tick()
  await Promise.resolve()
  await tick()
  return target
}

describe('NotificationsSection a11y', () => {
  it('default state (FDA granted) has no a11y violations', async () => {
    const target = await mountSection()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('FDA-pending state (Downloads card greyed) has no a11y violations', async () => {
    setStatus(true)
    const target = await mountSection()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
