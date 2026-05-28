/**
 * Tier-3 a11y tests for `FileSystemWatchingSection.svelte`.
 *
 * Covered states: default (FDA granted), FDA pending. Functional behavior
 * (sub-group structure, anchor id, ToggleGroup write-through, IPC fire) is
 * pinned in the companion `.svelte.test.ts` file.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const {
  getSettingMock,
  setSettingMock,
  downloadsWatcherStatusMock,
  recheckGateMock,
  setGlobalRevealShortcutMock,
  getIndexStatusMock,
  clearDriveIndexMock,
} = vi.hoisted(() => ({
  getSettingMock: vi.fn(),
  setSettingMock: vi.fn(),
  downloadsWatcherStatusMock: vi.fn(),
  recheckGateMock: vi.fn(),
  setGlobalRevealShortcutMock: vi.fn(),
  getIndexStatusMock: vi.fn(),
  clearDriveIndexMock: vi.fn(),
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
    setGlobalRevealShortcut: setGlobalRevealShortcutMock,
    getIndexStatus: getIndexStatusMock,
    clearDriveIndex: clearDriveIndexMock,
  },
}))

import FileSystemWatchingSection from './FileSystemWatchingSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function setDefaultSettings(): void {
  getSettingMock.mockImplementation((key: string): unknown => {
    switch (key) {
      case 'indexing.enabled':
        return true
      case 'behavior.fileSystemWatching.downloadsNotifications':
        return 'in-app'
      case 'behavior.fileSystemWatching.globalRevealShortcut.enabled':
        return true
      case 'behavior.fileSystemWatching.globalRevealShortcut.binding':
        return '\u{2303}\u{2325}\u{2318}J'
      case 'behavior.fileSystemWatching.globalRevealShortcut.acknowledged':
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
  setGlobalRevealShortcutMock.mockReset().mockResolvedValue({
    status: 'ok',
    data: { status: 'registered', binding: '\u{2303}\u{2325}\u{2318}J', enabled: true },
  })
  getIndexStatusMock.mockReset().mockResolvedValue({ status: 'ok', data: { dbFileSize: 1024 } })
  clearDriveIndexMock.mockReset().mockResolvedValue({ status: 'ok', data: null })

  setDefaultSettings()
  setStatus(false)
})

async function mountSection(): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(FileSystemWatchingSection, { target, props: { searchQuery: '' } })
  // Drain the onMount IPC chain; jsdom needs a few flushes.
  await tick()
  await Promise.resolve()
  await tick()
  await Promise.resolve()
  await tick()
  return target
}

describe('FileSystemWatchingSection a11y', () => {
  it('default state (FDA granted) has no a11y violations', async () => {
    const target = await mountSection()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('FDA-pending state (sub-groups greyed) has no a11y violations', async () => {
    setStatus(true)
    const target = await mountSection()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
