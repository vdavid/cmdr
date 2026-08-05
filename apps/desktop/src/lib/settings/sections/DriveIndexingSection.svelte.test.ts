/**
 * Tier-3 tests for `DriveIndexingSection.svelte` (`Indexing > Drive indexing`).
 *
 * Pins the contract:
 *   - One card renders under the "Drive indexing" section title, holding the
 *     indexing toggle, the index-size / clear-index action row, the per-drive
 *     prompt toggle + re-enable button, and the stale-notify toggle.
 *   - The clear-index button calls the backend IPC.
 *   - The size and the clear button work with the master switch OFF, since a
 *     search walks whatever folder it's pointed at and leaves an index behind
 *     (`docs/specs/unindexed-search-plan.md` M10).
 *   - The hidden `indexing.indexSize` search anchor keeps the card visible when
 *     searching "index size", so the page never blanks.
 *
 * The section calls two backend IPCs (the index's disk use, clear index). Both
 * mocked so the tests run without a Tauri runtime.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const { getSettingMock, setSettingMock, getIndexDiskUsageMock, clearDriveIndexMock } = vi.hoisted(() => ({
  getSettingMock: vi.fn(),
  setSettingMock: vi.fn(),
  getIndexDiskUsageMock: vi.fn(),
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
    getIndexDiskUsage: getIndexDiskUsageMock,
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
  getIndexDiskUsageMock.mockReset().mockResolvedValue({ status: 'ok', data: 1024 })
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

  it('shows the size and offers Clear with drive indexing off, because a search still writes an index', async () => {
    // The master switch stops BACKGROUND indexing, never a search's walk
    // (Decision 13), so "off" is exactly the state where an index accumulates
    // that nobody asked for. It has to be visible and clearable there, or the
    // only people with an unwanted index are the ones who can't get rid of it.
    getSettingMock.mockImplementation((key: string): unknown => (key === 'indexing.enabled' ? false : '[]'))
    getIndexDiskUsageMock.mockResolvedValue({ status: 'ok', data: 42_000_000 })
    const target = await mountSection()

    expect(target.querySelector('.info-value')?.textContent).toContain('MB')
    const clearButton = Array.from(target.querySelectorAll('button')).find(
      (b) => b.textContent.trim() === 'Clear index',
    )
    expect(clearButton).toBeDefined()
    clearButton?.click()
    await tick()
    await Promise.resolve()
    expect(clearDriveIndexMock).toHaveBeenCalled()
    target.remove()
  })

  it('says "No index" and offers no Clear when nothing is on disk', async () => {
    getIndexDiskUsageMock.mockResolvedValue({ status: 'ok', data: 0 })
    const target = await mountSection()

    expect(target.querySelector('.info-value')?.textContent.trim()).toBe('No index')
    expect(Array.from(target.querySelectorAll('button')).map((b) => b.textContent.trim())).not.toContain('Clear index')
    target.remove()
  })

  it('keeps the card visible when searching "index size" (hidden anchor)', async () => {
    const target = await mountSection('index size')
    expect(target.querySelectorAll('.section-card')).toHaveLength(1)
    expect(target.textContent).toContain('Index size')
    target.remove()
  })

  it('leaves the per-drive rows fully live while drive indexing is on', async () => {
    const target = await mountSection()
    expect(target.querySelector('.master-off-note')).toBeNull()
    expect(target.querySelectorAll('.setting-row.disabled')).toHaveLength(0)
    expect(target.querySelector('.reenable-row.overridden')).toBeNull()
    target.remove()
  })

  it('marks the per-drive rows overridden, with one explanation, when drive indexing is off', async () => {
    // The master switch is a hard gate in the backend, so the rows it overrides
    // must LOOK overridden instead of pretending to work. The per-drive choices
    // themselves are untouched, which the note says out loud.
    getSettingMock.mockImplementation((key: string): unknown => {
      switch (key) {
        case 'indexing.enabled':
          return false
        case 'indexing.askForEachDrive':
        case 'indexing.staleNotify':
          return true
        case 'indexing.silencedDrives':
          return '[]'
        default:
          return undefined
      }
    })
    const target = await mountSection()

    const note = target.querySelector('.master-off-note')?.textContent.trim() ?? ''
    expect(note).toContain('Drive indexing is off')
    expect(note).toContain('keeps its own on or off choice')

    // Both per-drive rows dim and carry the badge; the hand-rendered re-enable row
    // dims with them rather than staying bright beside them.
    expect(target.querySelectorAll('.setting-row.disabled')).toHaveLength(2)
    expect(target.querySelectorAll('.disabled-badge')).toHaveLength(2)
    expect(target.querySelector('.reenable-row.overridden')).not.toBeNull()

    // Every switch below the master one is inert.
    const switches = Array.from(target.querySelectorAll<HTMLInputElement>('.setting-row.disabled input'))
    expect(switches).toHaveLength(2)
    for (const input of switches) expect(input.disabled).toBe(true)

    target.remove()
  })
})
