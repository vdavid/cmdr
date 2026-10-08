/**
 * Tier-3 tests for `ListingSection.svelte` (Appearance › Listing).
 *
 * Guards the trap the settings system is built around: a registry entry alone
 * doesn't render. `listing.showHiddenFiles` is reachable from the View menu and
 * `⌘⇧.` too, so a missing row here wouldn't break anything visibly — it would
 * just quietly leave the Settings page without the toggle. This pins the row,
 * its position (first in the card, above "Sort folders"), and that flipping
 * it writes the setting the panes and the menu both read. Also pins the note
 * that explains why "Sort folders" is greyed out while folders mix with files.
 *
 * The settings-store is stubbed so the section mounts without real IPC.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ListingSection from './ListingSection.svelte'
import { setSetting } from '$lib/settings/settings-store'

const stubs = vi.hoisted(() => ({ foldersFirst: true }))

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'listing.foldersFirst') return stubs.foldersFirst
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
  it('renders the row first in the card, above "Sort folders"', async () => {
    const target = await mountSection()
    const labels = rowLabels(target)
    expect(labels[0]).toBe('Show hidden files')
    expect(labels.indexOf('Show hidden files')).toBeLessThan(labels.indexOf('Sort folders'))
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

describe('ListingSection: "Sort folders" disabled note', () => {
  const noteSelector = '#setting-listing\\.directorySortMode-disabled-note'

  it('explains the greyed-out row while folders mix with files', async () => {
    stubs.foldersFirst = false
    const target = await mountSection()
    const note = target.querySelector(noteSelector)
    expect(note?.textContent.trim()).toBe(
      'Currently disabled because folders are mixed with files. Turn on “Show folders first” to enable this setting.',
    )
    const group = target.querySelector('[aria-label="Sort folders"]')
    expect(group?.getAttribute('aria-describedby')).toBe(note?.id)
    target.remove()
    stubs.foldersFirst = true
  })

  it('shows no note while folders come first', async () => {
    const target = await mountSection()
    expect(target.querySelector(noteSelector)).toBeNull()
    expect(target.querySelector('[aria-label="Sort folders"]')?.hasAttribute('aria-describedby')).toBe(false)
    target.remove()
  })
})
