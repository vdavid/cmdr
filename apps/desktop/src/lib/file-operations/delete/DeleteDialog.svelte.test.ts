/**
 * `DeleteDialog.svelte`: what hovering a shortened path gets you.
 *
 * The dialog is a fixed 500 px wide, so a long name ellipsizes; before this, the full
 * name was simply unreachable — no tooltip, and the panel couldn't be widened either.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import DeleteDialog from './DeleteDialog.svelte'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  startScanPreview: vi.fn(() => Promise.resolve({ previewId: 'preview-1' })),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: vi.fn((n: number | undefined) => (n === undefined ? '' : `${String(n)} B`)),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

const LONG_NAME = '2025-08-19 Szakál Adri sole pain exercise help.cmdr-tmp-0f3a91.mp4'
const FOLDER = '/Volumes/naspi/_todo_pics/Meet Recordings'

/** happy-dom lays nothing out, so overflow has to be stated for `overflowOnly` to mean anything. */
function setOverflowing(el: Element, overflowing: boolean): void {
  Object.defineProperty(el, 'scrollWidth', { value: overflowing ? 400 : 100, configurable: true })
  Object.defineProperty(el, 'clientWidth', { value: 100, configurable: true })
  vi.spyOn(el as HTMLElement, 'getBoundingClientRect').mockReturnValue({
    left: 100,
    top: 100,
    right: 200,
    bottom: 120,
    width: 100,
    height: 20,
    x: 100,
    y: 100,
    toJSON: () => ({}),
  })
}

function hover(el: Element): HTMLElement | null {
  el.dispatchEvent(new MouseEvent('mouseenter'))
  vi.advanceTimersByTime(500)
  return document.querySelector('.cmdr-tooltip.visible')
}

function mountDialog(folder: string): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(DeleteDialog, {
    target,
    props: {
      sourceItems: [
        { name: LONG_NAME, isDirectory: false, isSymlink: false, size: 385875968 },
        { name: 'notes.txt', isDirectory: false, isSymlink: false, size: 12 },
      ],
      sourcePaths: [`${folder}/${LONG_NAME}`, `${folder}/notes.txt`],
      sourceFolderPath: folder,
      isPermanent: true,
      supportsTrash: false,
      isFromCursor: true,
      sortColumn: 'name',
      sortOrder: 'ascending',
      sourceVolumeId: 'root',
      onConfirm: () => {},
      onCancel: () => {},
    },
  })
  return target
}

describe('DeleteDialog path tooltips', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('hands over the full path when a row name is cut short', async () => {
    const target = mountDialog(FOLDER)
    await tick()

    const rows = target.querySelectorAll('.item-name')
    setOverflowing(rows[0], true)

    expect(hover(rows[0])?.textContent).toBe(`${FOLDER}/${LONG_NAME}`)
  })

  it('stays quiet on a row that fits', async () => {
    const target = mountDialog(FOLDER)
    await tick()

    const rows = target.querySelectorAll('.item-name')
    setOverflowing(rows[1], false)

    expect(hover(rows[1])).toBeNull()
  })

  it('spells out a home path that the "From" line abbreviates to ~', async () => {
    const target = mountDialog('/Users/veszelovszki/projects-git/vdavid/cmdr')
    await tick()

    const from = target.querySelectorAll('.source-path')[0]
    expect(from.textContent).toContain('~/projects-git/vdavid/cmdr')
    setOverflowing(from, false)

    expect(hover(from)?.textContent).toBe('/Users/veszelovszki/projects-git/vdavid/cmdr')
  })
})
