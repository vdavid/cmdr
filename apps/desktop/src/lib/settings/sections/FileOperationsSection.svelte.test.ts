/**
 * Tier-3 tests for `FileOperationsSection.svelte` (Behavior › File operations).
 *
 * Pins two things:
 *   1. The two `showInAdvanced` mirror rows (`fileOperations.maxConflictsToShow`,
 *      `fileOperations.progressUpdateInterval`) render on this page. They're
 *      load-bearing: if they didn't render here, a globally-searchable Advanced
 *      would match this page and then show a blank section.
 *   2. Card grouping under search: a search matching only the Renaming card's
 *      row leaves NO empty "Conflicts and progress" frame standing (card
 *      visibility is section-owned via `anyVisible`).
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import FileOperationsSection from './FileOperationsSection.svelte'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'fileOperations.allowFileExtensionChanges') return 'ask'
    if (key === 'fileOperations.progressUpdateInterval') return 500
    if (key === 'fileOperations.maxConflictsToShow') return 100
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
  mount(FileOperationsSection, { target, props: { searchQuery } })
  await tick()
  return target
}

function cardLabels(target: HTMLElement): string[] {
  return Array.from(target.querySelectorAll('.section-card-label')).map((el) => el.textContent.trim())
}

describe('FileOperationsSection card groups', () => {
  it('renders both cards with no search', async () => {
    const target = await mountSection()
    expect(cardLabels(target)).toEqual(expect.arrayContaining(['Renaming', 'Conflicts and progress']))
    target.remove()
  })

  it('renders the two mirrored Conflicts-and-progress rows on this page', async () => {
    const target = await mountSection()
    // `SettingRow` renders `<label for={id}>`, so each row's `for` identifies its setting.
    const labelFors = Array.from(target.querySelectorAll('label.setting-label')).map((el) => el.getAttribute('for'))
    expect(labelFors).toContain('fileOperations.maxConflictsToShow')
    expect(labelFors).toContain('fileOperations.progressUpdateInterval')
    target.remove()
  })

  it('shows only the Renaming card when searching an extension term, leaving no empty cards', async () => {
    const target = await mountSection('extension')
    const labels = cardLabels(target)
    expect(labels).toContain('Renaming')
    expect(labels).not.toContain('Conflicts and progress')
    expect(target.querySelectorAll('.section-card')).toHaveLength(1)
    target.remove()
  })
})
