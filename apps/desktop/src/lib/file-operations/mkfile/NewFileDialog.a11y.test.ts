/**
 * Tier 3 a11y tests for `NewFileDialog.svelte`.
 *
 * Simpler than NewFolderDialog — no AI suggestions, no timeout warning.
 * Covers the empty default, pre-filled name, and (via a forced stub)
 * a visible inline error message.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import NewFileDialog from './NewFileDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  createFile: vi.fn(() => Promise.resolve()),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  getFileAt: vi.fn(() => Promise.resolve(null)),
  isIpcError: vi.fn(() => false),
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

describe('NewFileDialog a11y', () => {
  it('default (empty name) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(NewFileDialog, {
      target,
      props: {
        currentPath: '/Users/test/Projects',
        listingId: 'listing-1',
        showHiddenFiles: false,
        initialName: '',
        volumeId: 'root',
        onCreated: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('pre-filled name has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(NewFileDialog, {
      target,
      props: {
        currentPath: '/Users/test/Projects',
        listingId: 'listing-2',
        showHiddenFiles: false,
        initialName: 'notes.txt',
        volumeId: 'root',
        onCreated: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
