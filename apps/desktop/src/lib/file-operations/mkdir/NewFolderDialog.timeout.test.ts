/**
 * The mkdir timeout recovery path: `create_directory` came back timed out, so the
 * dialog offers "Refresh" instead of pretending the folder is there.
 *
 * The refresh it fires is deliberately UNFORCED. It's a top-up after a write, and
 * on a watcher-backed volume (MTP, SMB) the mutation pipeline has already patched
 * the cache, so forcing a full re-read here would cost ~17 s on a 1k-entry MTP
 * folder right after the user already waited out a timeout. Only an explicit ⌘R
 * or the MCP `refresh` tool forces.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import NewFolderDialog from './NewFolderDialog.svelte'
import { createDirectory, refreshListing } from '$lib/tauri-commands'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  createDirectory: vi.fn(() => Promise.resolve()),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  getAiStatus: vi.fn(() => Promise.resolve('unavailable')),
  getFileAt: vi.fn(() => Promise.resolve(null)),
  getFolderSuggestions: vi.fn(() => Promise.resolve([])),
  streamFolderSuggestions: vi.fn(() => ({ promise: Promise.resolve(), cancel: () => Promise.resolve() })),
  // The dialog only branches on `timedOut`, and the rejection below carries it.
  isIpcError: vi.fn(() => true),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  refreshListing: vi.fn(() => Promise.resolve({ data: null, timedOut: false })),
}))

/** Mounts the dialog with a name already typed in, so OK is enabled on first render. */
function mountDialog(onCancel: () => void): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(NewFolderDialog, {
    target,
    props: {
      currentPath: '/Users/test/Projects',
      listingId: 'listing-1',
      showHiddenFiles: false,
      initialName: 'photos',
      volumeId: 'root',
      onCreated: () => {},
      onCancel,
    },
  })
  return target
}

/** Settles the dialog's validation debounce plus the awaited `createDirectory`. */
async function settle(): Promise<void> {
  for (let i = 0; i < 12; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

describe('NewFolderDialog timeout recovery', () => {
  it('refreshes the listing WITHOUT forcing when the user takes the timeout way out', async () => {
    vi.mocked(createDirectory).mockRejectedValueOnce({ message: 'still working on it', timedOut: true })
    const onCancel = vi.fn()
    const target = mountDialog(onCancel)
    await settle()

    // Enter in the name field is the same `handleConfirm` the OK button calls.
    target.querySelector('input')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await settle()

    const timeoutActions = target.querySelector('.timeout-actions')
    expect(timeoutActions, 'the timeout warning renders after a timed-out create').not.toBeNull()
    const refreshButton = timeoutActions?.querySelector('button')
    refreshButton?.click()
    await settle()

    expect(refreshListing).toHaveBeenCalledExactlyOnceWith('listing-1', false)
    expect(onCancel).toHaveBeenCalledOnce()
  })
})
