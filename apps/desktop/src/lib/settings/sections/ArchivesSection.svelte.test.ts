/**
 * Tier-3 tests for `ArchivesSection.svelte`.
 *
 * The section is registry-driven: every row is a `SettingRow` over a registry id, so
 * what's worth pinning is that each of the three Enter-behavior formats has a row, that
 * each row sits in the card it claims, and that a change lands on that row's OWN
 * setting id (one blob shared by three rows was the shape this replaced).
 *
 * Plus the compression-level contract:
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

function setDefaultSettings(level = 6, enter: Partial<Record<'zip' | 'ooxml' | 'bundle', string>> = {}): void {
  getSettingMock.mockImplementation((key: string): unknown => {
    if (key === 'behavior.archiveEnter.zip') return enter.zip ?? 'ask'
    if (key === 'behavior.archiveEnter.ooxml') return enter.ooxml ?? 'open'
    if (key === 'behavior.archiveEnter.bundle') return enter.bundle ?? 'ask'
    if (key === 'behavior.archiveCompressionLevel') return level
    return undefined
  })
}

/** The row labels rendered inside the card titled `cardLabel`. */
function rowLabelsInCard(target: HTMLElement, cardLabel: string): string[] {
  const card = Array.from(target.querySelectorAll('.section-card-wrap')).find(
    (el) => el.querySelector('.section-card-label')?.textContent.trim() === cardLabel,
  )
  if (!card) throw new Error(`No card labelled "${cardLabel}"`)
  return Array.from(card.querySelectorAll('.setting-label')).map((el) => el.textContent.trim())
}

/** The pressed option of the toggle group inside the row labelled `rowLabel`. */
function selectedOption(target: HTMLElement, rowLabel: string): string | undefined {
  const row = Array.from(target.querySelectorAll('.setting-row')).find(
    (el) => el.querySelector('.setting-label')?.textContent.trim() === rowLabel,
  )
  if (!row) throw new Error(`No row labelled "${rowLabel}"`)
  const on = row.querySelector('.tg-item[data-state="on"]')
  return on?.textContent.trim()
}

/** Clicks the option button reading `optionLabel` inside the row labelled `rowLabel`. */
function clickOption(target: HTMLElement, rowLabel: string, optionLabel: string): void {
  const row = Array.from(target.querySelectorAll('.setting-row')).find(
    (el) => el.querySelector('.setting-label')?.textContent.trim() === rowLabel,
  )
  if (!row) throw new Error(`No row labelled "${rowLabel}"`)
  const button = Array.from(row.querySelectorAll('.tg-item')).find((el) => el.textContent.trim() === optionLabel)
  if (!button) throw new Error(`No "${optionLabel}" option in "${rowLabel}"`)
  ;(button as HTMLElement).click()
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

describe('ArchivesSection Enter behavior rows', () => {
  it('gives each format its own row, in the card it belongs to', async () => {
    const target = await mountSection()

    // Zip and the zip-based documents share the Archives card (both are things Cmdr
    // browses into); bundles are their own card because they're folders, not files.
    expect(rowLabelsInCard(target, 'Archives')).toEqual(['Zip archives', 'Documents and packages', 'Compression level'])
    expect(rowLabelsInCard(target, 'App bundles')).toEqual(['App bundles'])

    target.remove()
  })

  it('seeds each row from its own setting', async () => {
    setDefaultSettings(6, { zip: 'browse', ooxml: 'ask', bundle: 'open' })
    const target = await mountSection()

    expect(selectedOption(target, 'Zip archives')).toBe('Browse')
    expect(selectedOption(target, 'Documents and packages')).toBe('Ask')
    expect(selectedOption(target, 'App bundles')).toBe('Open')

    target.remove()
  })

  it("writes a change to that row's own id, and to nothing else", async () => {
    const target = await mountSection()

    clickOption(target, 'Zip archives', 'Browse')

    expect(setSettingMock).toHaveBeenCalledExactlyOnceWith('behavior.archiveEnter.zip', 'browse')
    target.remove()
  })

  it('shows the Office documents row for a search that only matches it', async () => {
    // The row is findable on its own terms now, which is what a registry entry buys:
    // the blob it replaced had one label for all three formats.
    const target = await mountSection('docx')

    expect(rowLabelsInCard(target, 'Archives')).toEqual(['Documents and packages'])
    // And the App bundles card is gone rather than standing empty.
    expect(Array.from(target.querySelectorAll('.section-card-label')).map((el) => el.textContent.trim())).toEqual([
      'Archives',
    ])

    target.remove()
  })
})

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
    // "bundle" matches the bundle row but not the compression-level keywords.
    const target = await mountSection('bundle')
    expect(target.querySelector('[role="slider"]')).toBeNull()
    const rowLabels = Array.from(target.querySelectorAll('.setting-label')).map((el) => el.textContent.trim())
    expect(rowLabels).not.toContain('Compression level')
    target.remove()
  })
})
