/**
 * Settings search finds a row by its own OS's words: the label, description, and keywords are
 * platform blocks, and the index reads the branch this window's OS renders. So a Windows user
 * searching "Recycle Bin" finds the delete-confirmation rows, a Linux or Mac user finds them by
 * "trash", and neither finds them by the other OS's word.
 */
import { afterEach, describe, expect, it } from 'vitest'
import type { Platform } from '#lib/platform/index.js'
import { withEveryCapability, withPlatform } from '#lib/platform/test-platform.js'
import { clearSearchIndex, searchSettings } from './settings-search'

withEveryCapability()

/** The rows whose copy names the Trash: F8 without asking, and the progress window's keywords. */
const TRASH_ROWS = ['fileOperations.skipConfirmation', 'fileOperations.keepProgressOpenWhenDone']

function idsOn(platform: Platform, query: string): string[] {
  withPlatform(platform)
  // The index is cached per language; the OS is fixed per window, so a test that flips it rebuilds.
  clearSearchIndex()
  return searchSettings(query).map((r) => r.entry.id)
}

afterEach(() => {
  withPlatform('linux')
  clearSearchIndex()
})

describe('searching for the place deleted files go', () => {
  it.each(TRASH_ROWS)('finds %s by “Recycle Bin” on Windows, and not by “trash”', (id) => {
    expect(idsOn('windows', 'Recycle Bin')).toContain(id)
    expect(idsOn('windows', 'recycle bin')).toContain(id)
    expect(idsOn('windows', 'trash')).not.toContain(id)
  })

  it.each([['linux'], ['macos']] as const)('finds both rows by “trash” on %s, and not by “Recycle Bin”', (platform) => {
    const byTrash = idsOn(platform, 'Trash')
    const byRecycleBin = idsOn(platform, 'Recycle Bin')
    for (const id of TRASH_ROWS) {
      expect(byTrash).toContain(id)
      expect(byRecycleBin).not.toContain(id)
    }
  })
})

describe('searching for the OS’s own words elsewhere in Settings', () => {
  it('finds the low disk space threshold by its OS’s name for the system drive', () => {
    expect(idsOn('windows', 'system drive')).toContain('behavior.fileSystemWatching.lowDiskSpaceThresholdPercent')
    expect(idsOn('macos', 'system drive')).not.toContain('behavior.fileSystemWatching.lowDiskSpaceThresholdPercent')
    expect(idsOn('macos', 'startup disk')).toContain('behavior.fileSystemWatching.lowDiskSpaceThresholdPercent')
  })
})
