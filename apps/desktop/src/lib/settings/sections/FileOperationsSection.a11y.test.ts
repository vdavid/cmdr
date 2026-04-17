/**
 * Tier 3 a11y tests for `FileOperationsSection.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import FileOperationsSection from './FileOperationsSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'fileOperations.allowFileExtensionChanges') return 'ask'
    if (key === 'fileOperations.progressUpdateInterval') return 100
    if (key === 'fileOperations.maxConflictsToShow') return 200
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

describe('FileOperationsSection a11y', () => {
  it('default has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FileOperationsSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
