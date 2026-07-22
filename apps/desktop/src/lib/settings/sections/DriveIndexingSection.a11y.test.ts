/**
 * Tier-3 a11y tests for `DriveIndexingSection.svelte` (`Indexing > Drive
 * indexing`). Functional behavior (card structure, clear-index IPC, hidden
 * search anchor) is pinned in the companion `.svelte.test.ts` file.
 */

import { describe, it, vi, beforeEach } from 'vitest'
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
import { expectNoA11yViolations } from '$lib/test-a11y'

beforeEach(() => {
  getSettingMock.mockReset().mockImplementation((key: string): unknown => {
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
  setSettingMock.mockReset()
  getIndexStatusMock.mockReset().mockResolvedValue({ status: 'ok', data: { dbFileSize: 1024 } })
  clearDriveIndexMock.mockReset().mockResolvedValue({ status: 'ok', data: null })
})

async function mountSection(): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(DriveIndexingSection, { target, props: { searchQuery: '' } })
  await tick()
  await Promise.resolve()
  await tick()
  return target
}

describe('DriveIndexingSection a11y', () => {
  it('default state has no a11y violations', async () => {
    const target = await mountSection()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
