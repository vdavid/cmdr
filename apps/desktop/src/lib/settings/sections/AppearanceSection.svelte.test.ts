/**
 * Tier-3 tests for `AppearanceSection.svelte` (Appearance › Colors and formats).
 *
 * Pins the card grouping under search: the page renders four cards (Theme,
 * List coloring, Date and time, Pane tints), and a search that matches only one
 * card's rows leaves NO empty card frames standing. Card visibility is
 * section-owned via `{#if anyVisible(shouldShow, ...memberIds)}` over the SAME
 * `shouldShow` predicate the rows use, so an all-filtered-out card hides its
 * frame too.
 *
 * The settings-store is stubbed so the section mounts without real IPC.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import AppearanceSection from './AppearanceSection.svelte'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'theme.mode') return 'system'
    if (key === 'appearance.appColor') return 'system'
    if (key === 'appearance.sizeColors') return 'rainbow'
    if (key === 'appearance.dateColors') return 'app'
    if (key === 'appearance.dateTimeFormat') return 'iso'
    if (key === 'appearance.customDateTimeFormat') return 'YYYY-MM-DD HH:mm'
    if (key === 'listing.stripedRows') return false
    if (key === 'appearance.tintLocal') return 'none'
    if (key === 'appearance.tintSmb') return 'none'
    if (key === 'appearance.tintMtp') return 'none'
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/tauri-commands', () => ({
  openAppearanceSettings: vi.fn(() => Promise.resolve()),
  invoke: vi.fn(() => Promise.resolve()),
}))

async function mountSection(searchQuery = ''): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AppearanceSection, { target, props: { searchQuery } })
  await tick()
  return target
}

function cardLabels(target: HTMLElement): string[] {
  return Array.from(target.querySelectorAll('.section-card-label')).map((el) => el.textContent.trim())
}

describe('AppearanceSection card groups', () => {
  it('renders the four cards with no search', async () => {
    const target = await mountSection()
    expect(cardLabels(target)).toEqual(
      expect.arrayContaining(['Theme', 'List coloring', 'Date and time', 'Pane tints']),
    )
    target.remove()
  })

  it('renders only the matching card when searching, leaving no empty cards', async () => {
    // Pre-fix each `SectionCard` drew its frame unconditionally, so a search that
    // matched only the Theme rows still painted List coloring / Date and time /
    // Pane tints as empty boxes. The fix gates each card frame on
    // `anyVisible(shouldShow, ...memberIds)`, the SAME predicate the rows use, so
    // an all-filtered-out card hides too.
    const target = await mountSection('theme')
    const labels = cardLabels(target)
    expect(labels).toContain('Theme')
    expect(labels).not.toContain('List coloring')
    expect(labels).not.toContain('Date and time')
    expect(labels).not.toContain('Pane tints')
    // Exactly one card frame is left standing (the Theme card).
    expect(target.querySelectorAll('.section-card')).toHaveLength(1)
    target.remove()
  })
})
