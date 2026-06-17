/**
 * Tier-3 tests for `FileOperationsSection.svelte` (Behavior › File operations).
 *
 * The page holds ONLY `fileOperations.allowFileExtensionChanges` now: the
 * conflict/progress settings live in Advanced (their single home), never
 * mirrored here. So the page renders one unlabeled `SectionCard` with that one
 * row, and the former mirror rows must NOT appear.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import FileOperationsSection from './FileOperationsSection.svelte'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'fileOperations.allowFileExtensionChanges') return 'ask'
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

describe('FileOperationsSection', () => {
  it('renders a single unlabeled card holding only the renaming row', async () => {
    const target = await mountSection()
    expect(target.querySelectorAll('.section-card')).toHaveLength(1)
    // Unlabeled card: no `.section-card-label` heading.
    expect(cardLabels(target)).toEqual([])
    const labelFors = Array.from(target.querySelectorAll('label.setting-label')).map((el) => el.getAttribute('for'))
    expect(labelFors).toContain('fileOperations.allowFileExtensionChanges')
    target.remove()
  })

  it('does not render the former Advanced mirror rows', async () => {
    const target = await mountSection()
    const labelFors = Array.from(target.querySelectorAll('label.setting-label')).map((el) => el.getAttribute('for'))
    expect(labelFors).not.toContain('fileOperations.maxConflictsToShow')
    expect(labelFors).not.toContain('fileOperations.progressUpdateInterval')
    target.remove()
  })

  it('hides the card entirely when the search matches nothing on this page', async () => {
    const target = await mountSection('zzznomatch')
    expect(target.querySelectorAll('.section-card')).toHaveLength(0)
    target.remove()
  })
})
