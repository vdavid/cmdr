/**
 * Tier-3 tests for `FileSystemWatchingSection.svelte`.
 *
 * Pins the contract:
 *   - Four sub-groups render: Drive indexing, Downloads notifications, Go to
 *     latest download, Low disk space.
 *   - When the FDA gate is closed (`fda_pending` is true), sub-groups 2 and 3
 *     grey out and one shared hint appears. Low disk space stays interactive
 *     (statfs needs no TCC permission).
 *   - The downloads-notifications and low-disk-space ToggleGroups write
 *     through to the settings store.
 *   - The global-shortcut on/off toggle calls the backend IPC.
 *   - The Downloads notifications and Low disk space sub-groups carry stable
 *     anchor ids so deep-links can land on them.
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
  setGlobalGoToLatestShortcutMock,
  getIndexStatusMock,
  clearDriveIndexMock,
} = vi.hoisted(() => ({
  getSettingMock: vi.fn(),
  setSettingMock: vi.fn(),
  downloadsWatcherStatusMock: vi.fn(),
  recheckGateMock: vi.fn(),
  setGlobalGoToLatestShortcutMock: vi.fn(),
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
    setGlobalGoToLatestShortcut: setGlobalGoToLatestShortcutMock,
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
      case 'behavior.fileSystemWatching.globalGoToLatestShortcut.enabled':
        return true
      case 'behavior.fileSystemWatching.globalGoToLatestShortcut.binding':
        return '\u{2303}\u{2325}\u{2318}J'
      case 'behavior.fileSystemWatching.globalGoToLatestShortcut.acknowledged':
        return true
      case 'behavior.fileSystemWatching.lowDiskSpaceNotifications':
        return 'in-app'
      case 'behavior.fileSystemWatching.lowDiskSpaceThresholdPercent':
        return 5
      default:
        return undefined
    }
  })
}

function setStatus(fdaPending: boolean, running = true): void {
  downloadsWatcherStatusMock.mockResolvedValue({
    status: 'ok',
    data: { running, downloadsDir: '/Users/me/Downloads', fdaPending },
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
  return target
}

describe('FileSystemWatchingSection', () => {
  it('renders all four sub-groups when FDA is granted', async () => {
    const target = await mountSection()
    const labels = Array.from(target.querySelectorAll('.section-card-label')).map((el) => el.textContent.trim())
    expect(labels).toEqual(
      expect.arrayContaining(['Drive indexing', 'Downloads notifications', 'Go to latest download', 'Low disk space']),
    )
    // Section title.
    const title = target.querySelector('.section-title')?.textContent.trim()
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
      (b) => b.textContent.trim() === 'macOS notifications',
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
    setGlobalGoToLatestShortcutMock.mockClear()
    const checkbox = target.querySelector<HTMLInputElement>(
      'input[type="checkbox"][data-test="global-shortcut-enabled"]',
    )
    if (!checkbox) throw new Error('Global shortcut enable checkbox not found')
    checkbox.click()
    await tick()
    await Promise.resolve()
    await tick()

    expect(setGlobalGoToLatestShortcutMock).toHaveBeenCalled()
    // The same flip writes the enabled setting through the helper.
    expect(setSettingMock).toHaveBeenCalledWith('behavior.fileSystemWatching.globalGoToLatestShortcut.enabled', false)
    target.remove()
  })

  it('shows a Switch (not a binding text input) for the go-to-latest toggle', async () => {
    const target = await mountSection()
    // The toggle is an Ark Switch with a hidden checkbox; there is no binding
    // text input here anymore (that moved to Keyboard shortcuts). The Ark
    // NumberInput in the Low disk space sub-group also renders `type="text"`
    // (a spinbutton), so the assertion excludes it by scope.
    expect(target.querySelector('input[type="checkbox"][data-test="global-shortcut-enabled"]')).not.toBeNull()
    expect(target.querySelector('input[type="text"]:not([data-scope="number-input"])')).toBeNull()
    target.remove()
  })

  it('exposes the Low disk space anchor id so the toast deep-link can target it', async () => {
    const target = await mountSection()
    const anchor = target.querySelector('#settings-low-disk-space')
    expect(anchor).not.toBeNull()
    target.remove()
  })

  it('writes through to the settings store when the user picks a different low-disk-space mode', async () => {
    const target = await mountSection()
    // The Low disk space ToggleGroup's "Off" option (the downloads group has
    // no option labelled exactly "Off", so the lookup is unambiguous).
    const offButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Off')
    if (!offButton) throw new Error('Off toggle not found')
    offButton.click()
    await tick()

    expect(setSettingMock).toHaveBeenCalledWith('behavior.fileSystemWatching.lowDiskSpaceNotifications', 'off')
    target.remove()
  })

  it('greys out the threshold input when the low-disk-space warning is off', async () => {
    getSettingMock.mockImplementation((key: string): unknown => {
      if (key === 'behavior.fileSystemWatching.lowDiskSpaceNotifications') return 'off'
      if (key === 'behavior.fileSystemWatching.lowDiskSpaceThresholdPercent') return 5
      if (key === 'indexing.enabled') return true
      if (key === 'behavior.fileSystemWatching.downloadsNotifications') return 'in-app'
      if (key === 'behavior.fileSystemWatching.globalGoToLatestShortcut.enabled') return true
      if (key === 'behavior.fileSystemWatching.globalGoToLatestShortcut.binding') return '\u{2303}\u{2325}\u{2318}J'
      return undefined
    })
    const target = await mountSection()
    const thresholdInput = target.querySelector('input[data-scope="number-input"]')
    if (!thresholdInput) throw new Error('Threshold number input not found')
    expect(thresholdInput.hasAttribute('disabled')).toBe(true)
    target.remove()
  })

  it('does not gate the Low disk space sub-group on FDA', async () => {
    setStatus(true)
    const target = await mountSection()
    // The anchor div carries no data-gated attribute: statfs needs no TCC
    // permission, so the sub-group stays interactive while FDA is pending.
    const anchor = target.querySelector('#settings-low-disk-space')
    expect(anchor?.getAttribute('data-gated')).toBeNull()
    target.remove()
  })

  it('describes the toggle with the LIVE binding (updates the helper text on rebind)', async () => {
    const target = await mountSection()
    // The Go to latest download sub-group's description references the
    // current binding (⌃⌥⌘J by default).
    expect(target.textContent).toContain('\u{2303}\u{2325}\u{2318}J')
    expect(target.textContent).toMatch(/Press .* from any app to jump to your most recent download\./)
    target.remove()
  })

  it('calls recheckDownloadsWatcherGate on mount (belt-and-braces FDA re-check)', async () => {
    const target = await mountSection()
    expect(recheckGateMock).toHaveBeenCalled()
    target.remove()
  })
})
