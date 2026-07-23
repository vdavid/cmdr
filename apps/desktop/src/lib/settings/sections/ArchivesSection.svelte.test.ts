/**
 * Tier-3 tests for the compression-level control in `ArchivesSection.svelte`.
 *
 * Pins the contract:
 *   - A "Compression level" row inside the Archives card renders a slider
 *     (role="slider") seeded from the current `behavior.archiveCompressionLevel`
 *     setting, framed by the "Faster" and "Smaller" end labels.
 *   - The control writes back through `setSetting(id, ...)` by id (proven via
 *     the thumb double-click reset, so Settings and the dialog stay one value).
 *   - A search that excludes the row hides it (the slider is gone).
 *
 * The settings store is mocked so the tests run without a Tauri runtime.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const { getSettingMock, setSettingMock } = vi.hoisted(() => ({
  getSettingMock: vi.fn(),
  setSettingMock: vi.fn(),
}))

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: getSettingMock,
  setSetting: setSettingMock,
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

import ArchivesSection from './ArchivesSection.svelte'

function setDefaultSettings(level = 6): void {
  getSettingMock.mockImplementation((key: string): unknown => {
    if (key === 'behavior.archiveEnterBehavior') return '{}'
    if (key === 'behavior.archiveCompressionLevel') return level
    return undefined
  })
}

beforeEach(() => {
  getSettingMock.mockReset()
  setSettingMock.mockReset()
  setDefaultSettings()
})

async function mountSection(searchQuery = ''): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ArchivesSection, { target, props: { searchQuery } })
  await tick()
  return target
}

describe('ArchivesSection compression level', () => {
  it('renders the compression-level slider seeded from the setting, framed by Faster/Smaller', async () => {
    setDefaultSettings(9)
    const target = await mountSection()

    const rowLabels = Array.from(target.querySelectorAll('.setting-label')).map((el) => el.textContent.trim())
    expect(rowLabels).toContain('Compression level')

    const slider = target.querySelector('[role="slider"]')
    expect(slider).not.toBeNull()
    expect(slider?.getAttribute('aria-valuenow')).toBe('9')

    const endLabels = Array.from(target.querySelectorAll('.sl-ends span')).map((el) => el.textContent.trim())
    expect(endLabels).toEqual(['Faster', 'Smaller'])

    target.remove()
  })

  it('persists a change through setSetting by id (thumb double-click resets to the default)', async () => {
    // Seed away from the default so the reset is an observable write.
    setDefaultSettings(3)
    const target = await mountSection()

    const thumb = target.querySelector('.sl-thumb')
    if (!thumb) throw new Error('Slider thumb not found')
    thumb.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
    await tick()

    expect(setSettingMock).toHaveBeenCalledWith('behavior.archiveCompressionLevel', 6)
    target.remove()
  })

  it('hides the compression-level row when a search excludes it', async () => {
    // "bundle" matches the Enter-behavior rows but not the compression-level keywords.
    const target = await mountSection('bundle')
    expect(target.querySelector('[role="slider"]')).toBeNull()
    const rowLabels = Array.from(target.querySelectorAll('.setting-label')).map((el) => el.textContent.trim())
    expect(rowLabels).not.toContain('Compression level')
    target.remove()
  })
})
