/**
 * Tier 3 a11y tests for `SearchDialog.svelte`.
 *
 * The dialog pulls in several Tauri commands (prepareSearchIndex,
 * searchFiles, translateSearchQuery, etc.) and reactive settings. We
 * mock the IPC + settings boundary and then run axe against the three
 * macro-states that matter structurally:
 *   - AI disabled, index not ready (loading state)
 *   - AI disabled, index ready (default search UI)
 *   - AI enabled, index ready (AI prompt row + search UI)
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { writable } from 'svelte/store'
import SearchDialog from './SearchDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

// Tauri IPC surface used by SearchDialog + children.
vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  prepareSearchIndex: vi.fn(() => Promise.resolve({ ready: true, entryCount: 1234 })),
  searchFiles: vi.fn(() => Promise.resolve({ entries: [], totalCount: 0 })),
  releaseSearchIndex: vi.fn(() => Promise.resolve()),
  translateSearchQuery: vi.fn(() => Promise.resolve({ display: {}, query: {} })),
  parseSearchScope: vi.fn(() => Promise.resolve({ includePaths: [], excludePatterns: [] })),
  getSystemDirExcludes: vi.fn(() => Promise.resolve(['node_modules', 'target', '.git'])),
  onSearchIndexReady: vi.fn(() => Promise.resolve(() => {})),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
}))

let aiProvider: 'off' | 'local' | 'cloud' = 'off'
vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'ai.provider') return aiProvider
    return undefined
  }),
}))

vi.mock('$lib/indexing', () => ({
  isScanning: vi.fn(() => false),
  getEntriesScanned: vi.fn(() => 0),
}))

vi.mock('$lib/icon-cache', () => ({
  iconCacheVersion: writable(0),
}))

describe('SearchDialog a11y', () => {
  beforeEach(() => {
    aiProvider = 'off'
  })

  it('default state (AI off, index loading) has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchDialog, {
      target,
      props: {
        onNavigate: () => {},
        onClose: () => {},
        currentFolderPath: '/Users/test',
      },
    })
    await tick()
    // Don't await the IPC chain — we're auditing the first paint.
    await expectNoA11yViolations(target)
  })

  it('after index ready (AI off) has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchDialog, {
      target,
      props: {
        onNavigate: () => {},
        onClose: () => {},
        currentFolderPath: '/Users/test',
      },
    })
    // Flush microtasks so prepareSearchIndex resolves and isIndexReady flips.
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('AI enabled (cloud provider) has no violations', async () => {
    aiProvider = 'cloud'
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchDialog, {
      target,
      props: {
        onNavigate: () => {},
        onClose: () => {},
        currentFolderPath: '/Users/test',
      },
    })
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })
})
