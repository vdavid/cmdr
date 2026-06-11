/**
 * Tier 3 a11y tests for `SelectionDialog.svelte`.
 *
 * The dialog reuses the shared `QueryDialog`, so this suite audits the
 * Selection-specific configuration (title, banner, primary action label,
 * AI-on vs AI-off chip cardinality) against axe-core.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { writable } from 'svelte/store'
import SelectionDialog from './SelectionDialog.svelte'
import type { FileEntry } from '$lib/file-explorer/types'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  translateSelectionQuery: vi.fn(() =>
    Promise.resolve({
      pattern: null,
      kind: null,
      isDirectory: null,
      sizeMin: null,
      sizeMax: null,
      modifiedAfter: null,
      modifiedBefore: null,
      caveat: null,
      label: null,
    }),
  ),
  addRecentSelection: vi.fn(() => Promise.resolve()),
  removeRecentSelection: vi.fn(() => Promise.resolve()),
  getRecentSelections: vi.fn(() => Promise.resolve([])),
  showFileContextMenu: vi.fn(() => Promise.resolve()),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
}))

let aiProvider: 'off' | 'local' | 'cloud' = 'off'
vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'ai.provider') return aiProvider
    if (key === 'search.autoApply') return true
    return undefined
  }),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/icon-cache', () => ({
  iconCacheVersion: writable(0),
}))

function entry(name: string): FileEntry {
  return {
    name,
    path: `/folder/${name}`,
    parentPath: '/folder',
    isDirectory: false,
    isSymlink: false,
    permissions: 0o644,
    owner: 'me',
    group: 'staff',
    iconId: 'file',
    extendedMetadataLoaded: true,
  }
}

const ENTRIES: FileEntry[] = [entry('a.png'), entry('b.txt'), entry('c.png')]

describe('SelectionDialog a11y', () => {
  beforeEach(() => {
    aiProvider = 'off'
  })

  it("Select files (mode='add', AI off) has no violations", async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SelectionDialog, {
      target,
      props: {
        mode: 'add',
        entries: ENTRIES,
        cursorIndex: 0,
        isSnapshotPane: false,
        onCommit: () => {},
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it("Deselect files (mode='remove', AI cloud) has no violations", async () => {
    aiProvider = 'cloud'
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SelectionDialog, {
      target,
      props: {
        mode: 'remove',
        entries: ENTRIES,
        cursorIndex: 0,
        isSnapshotPane: false,
        onCommit: () => {},
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('snapshot-pane (R7 banner visible) has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SelectionDialog, {
      target,
      props: {
        mode: 'add',
        entries: ENTRIES,
        cursorIndex: 0,
        isSnapshotPane: true,
        onCommit: () => {},
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
