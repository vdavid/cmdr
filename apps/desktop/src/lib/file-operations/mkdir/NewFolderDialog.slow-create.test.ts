/**
 * A new folder on a slow volume (ERR-AREUV): past the backend's reply deadline
 * the dialog says it's still creating, never that it didn't, and closes like a
 * normal create once the folder lands.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import NewFolderDialog from './NewFolderDialog.svelte'
import { createDirectory, type MutationWaitOptions } from '$lib/tauri-commands'
import { MutationFailure } from '$lib/file-operations/mutation-error'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  createDirectory: vi.fn(() => Promise.resolve()),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  getAiStatus: vi.fn(() => Promise.resolve('unavailable')),
  getFileAt: vi.fn(() => Promise.resolve(null)),
  streamFolderSuggestions: vi.fn(() => ({ promise: Promise.resolve(), cancel: () => Promise.resolve() })),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
}))

/** Mounts the dialog with a name already typed in, so OK is enabled on first render. */
function mountDialog(onCreated: (name: string) => void): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(NewFolderDialog, {
    target,
    props: {
      currentPath: '/Volumes/naspi/photos',
      listingId: 'listing-1',
      showHiddenFiles: false,
      initialName: 'summer',
      volumeId: 'smb-naspi',
      onCreated,
      onCancel: () => {},
    },
  })
  return target
}

/** Settles the dialog's validation debounce plus the awaited create. */
async function settle(): Promise<void> {
  for (let i = 0; i < 12; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

/** A create that reports "still running" and then waits until the test ends it. */
function slowCreate() {
  let land!: () => void
  let refuse!: (e: unknown) => void
  vi.mocked(createDirectory).mockImplementationOnce(
    (_parent: string, _name: string, _volumeId?: string, _initiator?: unknown, wait?: MutationWaitOptions) => {
      wait?.onStillRunning?.()
      return new Promise<void>((resolve, reject) => {
        land = resolve
        refuse = reject
      })
    },
  )
  return {
    land: () => {
      land()
    },
    refuse: (e: unknown) => {
      refuse(e)
    },
  }
}

function pressEnter(target: HTMLElement): void {
  target.querySelector('input')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
}

describe('NewFolderDialog on a slow volume', () => {
  it('says it is still creating, then hands over to onCreated when the folder lands', async () => {
    const create = slowCreate()
    const onCreated = vi.fn()
    const target = mountDialog(onCreated)
    await settle()

    pressEnter(target)
    await settle()

    const notice = target.querySelector('[role="status"]')
    expect(notice?.textContent).toContain('Still creating “summer”')
    expect(target.textContent).not.toContain('Couldn’t')
    const buttons = [...target.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons, 'Cancel becomes Close: closing does not stop the create').toContain('Close')
    expect(onCreated).not.toHaveBeenCalled()

    create.land()
    await settle()
    expect(onCreated).toHaveBeenCalledExactlyOnceWith('summer')
  })

  it('shows the real refusal under the name when the slow create is refused', async () => {
    const create = slowCreate()
    const target = mountDialog(vi.fn())
    await settle()

    pressEnter(target)
    await settle()
    create.refuse(new MutationFailure({ type: 'alreadyExists', name: 'summer' }))
    await settle()

    expect(target.querySelector('[role="status"]')).toBeNull()
    expect(target.textContent).toContain('summer')
    const buttons = [...target.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons).toContain('Cancel')
  })
})
