/**
 * Tier-3 tests for `ListingSection.svelte` (Appearance › Listing).
 *
 * Guards the trap the settings system is built around: a registry entry alone
 * doesn't render. `listing.showHiddenFiles` is reachable from the View menu and
 * `⌘⇧.` too, so a missing row here wouldn't break anything visibly — it would
 * just quietly leave the Settings page without the toggle. This pins the row,
 * its position (first in the card, above "Sort directories"), and that flipping
 * it writes the setting the panes and the menu both read.
 *
 * The settings-store is stubbed so the section mounts without real IPC.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ListingSection from './ListingSection.svelte'
import { setSetting } from '$lib/settings/settings-store'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'listing.showHiddenFiles') return true
    if (key === 'appearance.useAppIconsForDocuments') return true
    if (key === 'appearance.showFunctionKeyBar') return true
    if (key === 'listing.directorySortMode') return 'likeFiles'
    if (key === 'listing.briefColumnWidthMode') return 'paneWidth'
    if (key === 'listing.briefColumnWidthMaxPx') return 400
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

async function mountSection(searchQuery = ''): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ListingSection, { target, props: { searchQuery } })
  await tick()
  return target
}

function rowLabels(target: HTMLElement): string[] {
  return Array.from(target.querySelectorAll('.setting-label')).map((el) => el.textContent.trim())
}

describe('ListingSection: show hidden files', () => {
  it('renders the row first in the card, above "Sort directories"', async () => {
    const target = await mountSection()
    const labels = rowLabels(target)
    expect(labels[0]).toBe('Show hidden files')
    expect(labels.indexOf('Show hidden files')).toBeLessThan(labels.indexOf('Sort directories'))
    target.remove()
  })

  it('surfaces the row when searching for "hidden"', async () => {
    const target = await mountSection('hidden')
    expect(rowLabels(target)).toContain('Show hidden files')
    target.remove()
  })

  it('writes `listing.showHiddenFiles` when the switch is flipped', async () => {
    const target = await mountSection()
    const input = target.querySelector<HTMLInputElement>('[role="switch"][aria-label="Show hidden files"]')
    expect(input).not.toBeNull()
    input?.click()
    await tick()
    expect(setSetting).toHaveBeenCalledWith('listing.showHiddenFiles', false)
    target.remove()
  })
})
