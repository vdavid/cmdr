/**
 * Tier-3 tests for `DriveIndexingSection.svelte` (`Indexing > Drive indexing`).
 *
 * Pins the contract:
 *   - One card renders under the "Drive indexing" section title, holding the
 *     indexing toggle, the index-size / clear-index action row, the per-drive
 *     prompt toggle + re-enable button, and the stale-notify toggle.
 *   - The clear-index button calls the backend IPC.
 *   - The hidden `indexing.indexSize` search anchor keeps the card visible when
 *     searching "index size", so the page never blanks.
 *
 * The section calls two backend IPCs (index status, clear index). Both mocked
 * so the tests run without a Tauri runtime.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const { getSettingMock, setSettingMock, getIndexStatusMock, clearDriveIndexMock } = vi.hoisted(() => ({
  getSettingMock: vi.fn(),
  setSettingMock: vi.fn(),
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
    getIndexStatus: getIndexStatusMock,
    clearDriveIndex: clearDriveIndexMock,
  },
}))

import DriveIndexingSection from './DriveIndexingSection.svelte'

function setDefaultSettings(): void {
  getSettingMock.mockImplementation((key: string): unknown => {
    switch (key) {
      case 'indexing.enabled':
        return true
      case 'indexing.askForEachDrive':
        return true
      case 'indexing.staleNotify':
        return true
      case 'indexing.silencedDrives':
        return '[]'
      default:
        return undefined
    }
  })
}

beforeEach(() => {
  getSettingMock.mockReset()
  setSettingMock.mockReset()
  getIndexStatusMock.mockReset().mockResolvedValue({ status: 'ok', data: { dbFileSize: 1024 } })
  clearDriveIndexMock.mockReset().mockResolvedValue({ status: 'ok', data: null })
  setDefaultSettings()
})

async function mountSection(searchQuery = ''): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(DriveIndexingSection, { target, props: { searchQuery } })
  await tick()
  await Promise.resolve()
  await tick()
  return target
}

describe('DriveIndexingSection', () => {
  it('renders the Drive indexing card under the section title', async () => {
    const target = await mountSection()
    const title = target.querySelector('.section-title')?.textContent.trim()
    expect(title).toBe('Drive indexing')
    // The index-size action row is present (its hidden anchor keeps it searchable).
    expect(target.textContent).toContain('Index size')
    expect(target.querySelectorAll('.section-card')).toHaveLength(1)
    target.remove()
  })

  it('calls the backend IPC when the clear-index button is clicked', async () => {
    const target = await mountSection()
    const clearButton = Array.from(target.querySelectorAll('button')).find(
      (b) => b.textContent.trim() === 'Clear index',
    )
    if (!clearButton) throw new Error('Clear index button not found')
    clearButton.click()
    await tick()
    await Promise.resolve()
    expect(clearDriveIndexMock).toHaveBeenCalled()
    target.remove()
  })

  it('keeps the card visible when searching "index size" (hidden anchor)', async () => {
    const target = await mountSection('index size')
    expect(target.querySelectorAll('.section-card')).toHaveLength(1)
    expect(target.textContent).toContain('Index size')
    target.remove()
  })
})
