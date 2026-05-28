/**
 * Tier-3 tests for `FileSystemWatchingSection.svelte` (M7).
 *
 * Pins the contract:
 *   - Three sub-groups render: Drive indexing, Downloads notifications, Reveal
 *     latest download.
 *   - When the FDA gate is closed (`fda_pending` is true), sub-groups 2 and 3
 *     grey out and one shared hint appears.
 *   - The downloads-notifications ToggleGroup writes through to the settings
 *     store.
 *   - The global-shortcut on/off toggle calls the backend IPC.
 *   - The Downloads notifications sub-group carries a stable anchor id so
 *     deep-links can land on it.
 *
 * The section calls a few backend IPCs (status snapshot, recheck gate, apply
 * shortcut, index status). All mocked so the tests can run without a Tauri
 * runtime.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
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

function setStatus(fdaPending: boolean, running = true): void {
  downloadsWatcherStatusMock.mockResolvedValue({
    status: 'ok',
    data: { running, downloadsDir: '/Users/me/Downloads', fdaPending, lastDetected: null },
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
  // The `onMount` chain awaits multiple promises (status, recheck, set shortcut).
  // Two `await tick()`s + a `Promise.resolve()` flush is enough on jsdom.
  await tick()
  await Promise.resolve()
  await tick()
  await Promise.resolve()
  await tick()
  return target as HTMLDivElement
}

describe('FileSystemWatchingSection', () => {
  it('renders all three sub-groups when FDA is granted', async () => {
    const target = await mountSection()
    const labels = Array.from(target.querySelectorAll('.section-card-label')).map((el) => el.textContent?.trim())
    expect(labels).toEqual(
      expect.arrayContaining(['Drive indexing', 'Downloads notifications', 'Reveal latest download']),
    )
    // Section title.
    const title = target.querySelector('.section-title')?.textContent?.trim()
    expect(title).toBe('File system watching')
    target.remove()
  })

  it('exposes the Downloads notifications anchor id so deep-links can target it', async () => {
    const target = await mountSection()
    const anchor = target.querySelector('#settings-downloads-notifications')
    expect(anchor).not.toBeNull()
    target.remove()
  })

  it('greys out sub-groups 2 and 3 with one shared hint when FDA is pending', async () => {
    setStatus(true)
    const target = await mountSection()
    // Exactly one FDA hint, not one per sub-group.
    const hints = target.querySelectorAll('.fda-hint')
    expect(hints).toHaveLength(1)
    expect(hints[0].textContent).toMatch(/Full Disk Access/)
    // Sub-group 2 and 3 carry `data-gated="true"` to make the visual state
    // assertable without sniffing class names. Sub-group 1 stays interactive
    // so the indexing toggle still works while FDA is pending.
    const gated = target.querySelectorAll('[data-gated="true"]')
    expect(gated).toHaveLength(2)
    target.remove()
  })

  it('writes through to the settings store when the user picks a different downloads-notifications mode', async () => {
    const target = await mountSection()
    // ToggleGroup renders one button per option with the visible label.
    const macosButton = Array.from(target.querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'macOS notifications',
    )
    if (!macosButton) throw new Error('macOS notifications toggle not found')
    macosButton.click()
    await tick()

    expect(setSettingMock).toHaveBeenCalledWith('behavior.fileSystemWatching.downloadsNotifications', 'macos')
    target.remove()
  })

  it('calls the backend IPC when the global-shortcut on/off toggle flips', async () => {
    const target = await mountSection()
    // Reset between the mount-time refreshShortcutStatus call and the toggle.
    setGlobalRevealShortcutMock.mockClear()
    const checkbox = target.querySelector(
      'input[type="checkbox"][data-test="global-shortcut-enabled"]',
    ) as HTMLInputElement | null
    if (!checkbox) throw new Error('Global shortcut enable checkbox not found')
    checkbox.checked = false
    checkbox.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    await Promise.resolve()
    await tick()

    expect(setGlobalRevealShortcutMock).toHaveBeenCalled()
    // The same flip writes the enabled setting through the helper.
    expect(setSettingMock).toHaveBeenCalledWith('behavior.fileSystemWatching.globalRevealShortcut.enabled', false)
    target.remove()
  })

  it('calls recheckDownloadsWatcherGate on mount (belt-and-braces FDA re-check)', async () => {
    const target = await mountSection()
    expect(recheckGateMock).toHaveBeenCalled()
    target.remove()
  })
})
