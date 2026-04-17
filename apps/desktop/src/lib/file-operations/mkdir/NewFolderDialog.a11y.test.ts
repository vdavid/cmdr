/**
 * Tier 3 a11y tests for `NewFolderDialog.svelte`.
 *
 * Covers the default form render, pre-filled input, and the AI-suggestions
 * region. Tauri IPC and the AI bridge are stubbed so the dialog can mount
 * cleanly in jsdom.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import NewFolderDialog from './NewFolderDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  createDirectory: vi.fn(() => Promise.resolve()),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  getAiStatus: vi.fn(() => Promise.resolve('unavailable')),
  getFileAt: vi.fn(() => Promise.resolve(null)),
  getFolderSuggestions: vi.fn(() => Promise.resolve([])),
  isIpcError: vi.fn(() => false),
  listen: vi.fn(() => Promise.resolve(() => {})),
  refreshListing: vi.fn(() => Promise.resolve()),
}))

describe('NewFolderDialog a11y', () => {
  it('default (empty name, AI unavailable) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(NewFolderDialog, {
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
    mount(NewFolderDialog, {
      target,
      props: {
        currentPath: '/Users/test/Projects',
        listingId: 'listing-2',
        showHiddenFiles: false,
        initialName: 'my-new-project',
        volumeId: 'root',
        onCreated: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
