/**
 * What the still-renaming toast says about the OTHER renames a slow volume is
 * still working on: "folders" when they're all folders, "files" when they're all
 * files, "files and folders" when they're mixed. Rendered through the real en
 * catalog, so the kind choice and the ICU sentence are checked together.
 */

import { describe, it, expect, beforeAll, afterAll, beforeEach } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { clearAllToasts, getToasts } from '$lib/ui/toast'
import { createChainReports } from './chain-reports'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})
beforeEach(() => {
  clearAllToasts()
})

const FILE = { isDirectory: false }
const FOLDER = { isDirectory: true }

/** Starts one never-ending slow rename per entry, oldest first, and returns the toast text. */
function stillRenamingText(entries: { originalName: string; isDirectory: boolean }[]): unknown {
  const reports = createChainReports({ paneId: 'left' })
  for (const entry of entries) void reports.stillRenaming(entry, new Promise<never>(() => {}))
  return getToasts()[0].content
}

describe('the still-renaming toast names the kind of the other renames', () => {
  it('says "other folders" when the others are all folders', () => {
    expect(
      stillRenamingText([
        { originalName: 'a', ...FOLDER },
        { originalName: 'b', ...FOLDER },
        { originalName: 'c.txt', ...FILE },
      ]),
    ).toBe('Still renaming “c.txt” and 2 other folders. The volume is slow to answer.')
  })

  it('says "other files" when the others are all files', () => {
    expect(
      stillRenamingText([
        { originalName: 'a.txt', ...FILE },
        { originalName: 'b.txt', ...FILE },
        { originalName: 'c', ...FOLDER },
      ]),
    ).toBe('Still renaming “c” and 2 other files. The volume is slow to answer.')
  })

  it('says "other files and folders" when the others are mixed', () => {
    expect(
      stillRenamingText([
        { originalName: 'a.txt', ...FILE },
        { originalName: 'b', ...FOLDER },
        { originalName: 'c.txt', ...FILE },
      ]),
    ).toBe('Still renaming “c.txt” and 2 other files and folders. The volume is slow to answer.')
  })

  it('says "one other folder" for a single other folder', () => {
    expect(
      stillRenamingText([
        { originalName: 'a', ...FOLDER },
        { originalName: 'b.txt', ...FILE },
      ]),
    ).toBe('Still renaming “b.txt” and one other folder. The volume is slow to answer.')
  })

  it('says "one other file" for a single other file', () => {
    expect(
      stillRenamingText([
        { originalName: 'a.txt', ...FILE },
        { originalName: 'b', ...FOLDER },
      ]),
    ).toBe('Still renaming “b” and one other file. The volume is slow to answer.')
  })
})
