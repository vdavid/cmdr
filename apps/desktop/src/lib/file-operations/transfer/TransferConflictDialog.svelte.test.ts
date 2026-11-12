/**
 * What the conflict body says about WHICH file it is asking about.
 *
 * The a11y sibling audits the same component's structure; this one is about the
 * question being answerable: a bare filename isn't, once two folders hold that
 * name.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import TransferConflictDialog from './TransferConflictDialog.svelte'
import type { WriteConflictEvent } from '#lib/tauri-commands/index.js'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

// Records each tooltip's text on its host, so a test can read what hovering would show.
vi.mock('#lib/tooltip/tooltip.js', async (importOriginal) => {
  const tooltipText = (param: unknown): string =>
    typeof param === 'string' ? param : ((param as { text?: string } | undefined)?.text ?? '')
  return {
    ...(await importOriginal<Record<string, unknown>>()),
    tooltip: (node: HTMLElement, param: unknown) => {
      node.dataset.tooltip = tooltipText(param)
      return {
        update: (next: unknown) => {
          node.dataset.tooltip = tooltipText(next)
        },
      }
    },
  }
})

function conflict(destinationPath: string): WriteConflictEvent {
  return {
    operationId: 'op-1',
    conflictId: 1,
    sourcePath: '/Users/test/set-0417/f001',
    destinationPath,
    sourceSize: { kind: 'exact', bytes: 2048 },
    destinationSize: { kind: 'exact', bytes: 1024 },
    sourceModified: 1_700_000_000,
    destinationModified: 1_699_000_000,
    destinationIsNewer: false,
    sizeDifference: -1024,
    sourceIsDirectory: false,
    destinationIsDirectory: false,
  }
}

async function mountDialog(conflictEvent: WriteConflictEvent): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(TransferConflictDialog, {
    target,
    props: {
      conflictEvent,
      isCopy: true,
      isMove: false,
      rollbackSupport: 'supported',
      isCancelling: false,
      isResolvingConflict: false,
      onResolve: () => {},
      onCancel: () => {},
    },
  })
  await tick()
  return target
}

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('the file the prompt is about', () => {
  it('names the folder under the filename, so two clashes over one name differ on screen', async () => {
    // A QA pass over 1,600 folders that each held an `f001` got 1,600 prompts
    // that read identically. The folder is the only thing that differs.
    const first = await mountDialog(conflict('/Volumes/Backup/2026/set-0417/f001'))
    expect(first.querySelector('.conflict-filename')?.textContent.trim()).toBe('f001')
    expect(first.querySelector('.conflict-folder')?.textContent).toBe('/Volumes/Backup/2026/set-0417')

    const second = await mountDialog(conflict('/Volumes/Backup/2026/set-0418/f001'))
    expect(second.querySelector('.conflict-folder')?.textContent).toBe('/Volumes/Backup/2026/set-0418')
  })

  it('shows the name alone when there is no folder to be sure of', async () => {
    // A relative path is a bug somewhere upstream, and a dialog asking about
    // the user's files is the wrong place to guess at one.
    const target = await mountDialog(conflict('f001'))
    expect(target.querySelector('.conflict-filename')?.textContent.trim()).toBe('f001')
    expect(target.querySelector('.conflict-folder')).toBeNull()
  })
})

describe('a clash with a name the destination spells differently', () => {
  // `café` composed and `café` decomposed print identically, so without a word
  // the prompt reads as an ordinary clash and nothing says an Overwrite lands
  // on an entry spelled another way.
  const HINT =
    'These names look the same, but the server spells them differently. Overwrite replaces the one that’s there.'

  it('says the names only look the same, and names the entry that is there', async () => {
    const target = await mountDialog({
      ...conflict('/Volumes/share/café.txt'),
      sourcePath: '/Users/test/café.txt',
      destinationIsLookAlike: true,
    })
    expect(target.querySelector('.conflict-look-alike')?.textContent.trim()).toBe(HINT)
    // The name on screen is the stored entry's own spelling: the one Overwrite
    // replaces, and the one that stays.
    expect(target.querySelector('.conflict-filename')?.textContent.trim()).toBe('café.txt')
  })

  it('says nothing extra for an ordinary clash', async () => {
    const target = await mountDialog(conflict('/Volumes/Backup/2026/set-0417/f001'))
    expect(target.querySelector('.conflict-look-alike')).toBeNull()
  })
})

describe('a name the destination refused', () => {
  const path = '/Volumes/testshare/music/a very long title.flac'

  it('says the name is too long, with no comparison of a file that isn’t there', async () => {
    const target = await mountDialog({ ...conflict(path), destinationSize: null, refusedName: 'tooLong' })
    expect(target.querySelector('.conflict-refused')?.textContent).toContain('can’t store a name this long')
    expect(target.querySelector('.conflict-comparison')).toBeNull()
    expect(target.querySelector('.conflict-filename')?.textContent.trim()).toBe('a very long title.flac')
  })

  it('says a name it can’t store for another reason isn’t allowed', async () => {
    const target = await mountDialog({ ...conflict('/Volumes/testshare/CON'), refusedName: 'notAllowed' })
    expect(target.querySelector('.conflict-refused')?.textContent).toContain('can’t store this name')
  })

  it('says nothing of the kind for an ordinary clash', async () => {
    const target = await mountDialog(conflict(path))
    expect(target.querySelector('.conflict-refused')).toBeNull()
  })
})

describe('a clash between a file and a folder', () => {
  // Overwrite on muscle memory is the risk: the type swap has to stand out in
  // BOTH directions, not just when a folder's contents are on the line.
  const path = '/Volumes/Backup/2026/report'

  it('warns when a file is about to replace a folder', async () => {
    const target = await mountDialog({ ...conflict(path), destinationIsDirectory: true })
    const warning = target.querySelector('.conflict-warning[role="alert"]')
    expect(warning?.textContent).toContain('Everything inside the folder would be gone')
  })

  it('warns when a folder is about to replace a file', async () => {
    const target = await mountDialog({ ...conflict(path), sourceIsDirectory: true })
    const warning = target.querySelector('.conflict-warning[role="alert"]')
    expect(warning?.textContent.trim()).toBe(
      'The target exists and is a file. You’re about to replace it with a folder by the same name. The file would be gone once the folder takes its place. What to do?',
    )
  })

  it('warns about neither for two files', async () => {
    const target = await mountDialog(conflict(path))
    expect(target.querySelector('.conflict-warning')).toBeNull()
  })
})

describe('a folder whose size the drive index has only partly counted', () => {
  // A folder mid-first-index can hold 40 GB while the index has counted 12 KB of
  // it. A plain Overwrite deletes that whole tree, so the prompt must never
  // present the partial count as the folder's size (#528).
  const path = '/Volumes/Backup/2026/report'
  const fileOverPartlyCountedFolder: WriteConflictEvent = {
    ...conflict(path),
    sourceIsDirectory: false,
    destinationIsDirectory: true,
    sourceSize: { kind: 'exact', bytes: 2048 },
    destinationSize: { kind: 'atLeast', bytes: 1024 },
    sizeDifference: null,
  }

  function existingSize(target: HTMLElement): string | undefined {
    return target.querySelector('.conflict-file')?.querySelector('.conflict-file-size')?.textContent.trim()
  }

  it('marks the folder size as a lower bound, the way the listing does', async () => {
    const target = await mountDialog(fileOverPartlyCountedFolder)
    expect(existingSize(target)).toBe('≥1.00 KiB')
  })

  it('never badges the incoming file larger against a lower bound', async () => {
    const target = await mountDialog(fileOverPartlyCountedFolder)
    expect(target.querySelector('.conflict-annotation.larger')).toBeNull()
  })

  it('offers no "Overwrite all smaller", which would compare against the floor', async () => {
    const target = await mountDialog(fileOverPartlyCountedFolder)
    const button = [...target.querySelectorAll('button')].find((b) => b.textContent.includes('Overwrite all smaller'))
    expect(button?.disabled).toBe(true)
  })

  it('shows an exact size bare', async () => {
    const target = await mountDialog(conflict(path))
    expect(existingSize(target)).toBe('1.00 KiB')
  })

  it('keeps the floor’s own tooltip, with no exact-bytes claim', async () => {
    const target = await mountDialog(fileOverPartlyCountedFolder)
    const size = target.querySelector<HTMLElement>('.conflict-file .conflict-file-size')
    expect(size?.dataset.tooltip).toContain('At least this much')
    expect(size?.dataset.tooltip).not.toContain('bytes')
  })
})

describe('two sizes that look the same', () => {
  // Two files a few bytes apart both read "1.18 MiB", and the choice feeds an
  // overwrite: the prompt must show they differ, or say they don't (#782).
  function sizesOf(target: HTMLElement): (string | undefined)[] {
    return [...target.querySelectorAll('.conflict-file-size')].map((el) => el.textContent.trim())
  }

  function pair(destinationBytes: number, sourceBytes: number): WriteConflictEvent {
    return {
      ...conflict('/Volumes/Backup/f001'),
      destinationSize: { kind: 'exact', bytes: destinationBytes },
      sourceSize: { kind: 'exact', bytes: sourceBytes },
      sizeDifference: destinationBytes - sourceBytes,
    }
  }

  it('shows exact bytes on both sides when the friendly sizes match but the bytes differ', async () => {
    const target = await mountDialog(pair(1_234_567, 1_234_570))
    expect(sizesOf(target)).toEqual(['1,234,567 bytes', '1,234,570 bytes'])
    expect(target.querySelector('.conflict-annotation.same')).toBeNull()
  })

  it('says "same size" when the bytes match', async () => {
    const target = await mountDialog(pair(1_234_567, 1_234_567))
    expect(sizesOf(target)).toEqual(['1.18 MiB', '1.18 MiB'])
    expect(target.querySelector('.conflict-annotation.same')?.textContent.trim()).toBe('(same size)')
  })

  it('keeps friendly sizes that already differ, with exact bytes on hover', async () => {
    const target = await mountDialog(pair(1024, 2048))
    expect(sizesOf(target)).toEqual(['1.00 KiB', '2.00 KiB'])
    const tooltips = [...target.querySelectorAll<HTMLElement>('.conflict-file-size')].map((el) => el.dataset.tooltip)
    expect(tooltips).toEqual(['1,024 bytes', '2,048 bytes'])
  })

  it('never says "same size" against an unknown size', async () => {
    const target = await mountDialog({ ...pair(1024, 1024), destinationSize: null, sizeDifference: null })
    expect(target.querySelector('.conflict-annotation.same')).toBeNull()
  })
})
