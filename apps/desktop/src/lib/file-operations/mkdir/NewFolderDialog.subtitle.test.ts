/**
 * The "Create folder in <dir>" subtitle names the folder the way the tab and the
 * header do. ❗ A share mounted at a disambiguated path (`/Volumes/public-1`) is
 * called by its own name: the subtitle said "public-1" while the tab said
 * "public" (QA round 5).
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import NewFolderDialog from './NewFolderDialog.svelte'

vi.mock('#lib/tauri-commands/index.js', () => ({
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

vi.mock('#lib/stores/volume-store.svelte.js', () => ({
  getVolumes: () => [
    {
      id: 'smb-p',
      name: 'public on localhost:11480',
      path: '/Volumes/public-1',
      category: 'attached_volume',
      isEjectable: false,
      rootLabel: 'public',
    },
  ],
}))

function subtitleIn(currentPath: string): string {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(NewFolderDialog, {
    target,
    props: {
      initiator: 'user',
      currentPath,
      listingId: 'listing-1',
      showHiddenFiles: false,
      initialName: '',
      volumeId: 'smb-p',
      onCreated: () => {},
      onCancel: () => {},
    },
  })
  const text = target.querySelector('.subtitle')?.textContent ?? ''
  target.remove()
  return text
}

describe('NewFolderDialog subtitle', () => {
  it('names a share’s root by the share, whatever mount dir it got', async () => {
    await tick()
    expect(subtitleIn('/Volumes/public-1')).toContain('public')
    expect(subtitleIn('/Volumes/public-1')).not.toContain('public-1')
  })

  it('names a folder inside it by the folder', () => {
    expect(subtitleIn('/Volumes/public-1/docs')).toContain('docs')
  })
})
