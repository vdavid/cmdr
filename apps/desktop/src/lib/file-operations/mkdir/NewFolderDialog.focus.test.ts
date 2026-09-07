/**
 * The F7 dialog must be typeable the instant it opens: `NewEntryNameField` focuses
 * (and selects) the name box on mount, and nothing in `ModalDialog` may take that
 * focus back to the scrim. Reported as vdavid/cmdr#84.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import NewFolderDialog from './NewFolderDialog.svelte'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  createDirectory: vi.fn(() => Promise.resolve()),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  getAiStatus: vi.fn(() => Promise.resolve('unavailable')),
  getFileAt: vi.fn(() => Promise.resolve(null)),
  streamFolderSuggestions: vi.fn(() => ({ promise: Promise.resolve(), cancel: () => Promise.resolve() })),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  refreshListing: vi.fn(() => Promise.resolve()),
}))

function mountDialog(initialName: string) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(NewFolderDialog, {
    target,
    props: {
      currentPath: '/Users/test/Projects',
      listingId: 'listing-1',
      showHiddenFiles: false,
      initialName,
      volumeId: 'root',
      onCreated: () => {},
      onCancel: () => {},
    },
  })
  return target
}

describe('NewFolderDialog focus', () => {
  it('focuses the name input on open, so typing starts immediately', async () => {
    const target = mountDialog('')
    await tick()
    await tick()

    const input = target.querySelector('input')
    expect(document.activeElement).toBe(input)
    target.remove()
  })

  it('selects a pre-filled name so typing replaces it', async () => {
    const target = mountDialog('my-project')
    await tick()
    await tick()

    const input = target.querySelector('input')
    expect(document.activeElement).toBe(input)
    expect(input?.selectionStart).toBe(0)
    expect(input?.selectionEnd).toBe('my-project'.length)
    target.remove()
  })
})
