/**
 * Closing the AI suggestions dialog with file rows on screen, over the REAL state slice.
 *
 * Regression anchor: `closeSuggestedOps` used to clear every review in the same tick it flipped
 * `open`. The rows' cells then re-read their op during the dialog's teardown (Ark's checkbox
 * reads its `checked` prop on the way out), found it gone, and threw, which took the whole main
 * window down to its crash panel. The E2E dialog-inset sweep caught it as a wedged app.
 */

import { describe, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import type { SuggestedOpPage, SuggestedSweepView } from '#lib/tauri-commands/index.js'

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<object>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  listSuggestedOps: () =>
    Promise.resolve([
      {
        sweepId: 1,
        createdAt: 1_780_000_000,
        rationale: 'Ten new files in Downloads.',
        groups: [
          {
            groupId: 7,
            sweepId: 1,
            verb: 'move',
            status: 'pending',
            displayName: 'Installers to the NAS',
            rationale: 'They all look like installers to me.',
            sourceVolumeId: 'root',
            sourceFolder: '/Users/someone/Downloads',
            destination: '/backups/Installers',
            destinationVolumeName: 'Naspolya',
            reversible: 'restoreMove',
            runsAs: null,
            destinationState: 'exists',
            liveOpCount: 3,
            totalOpCount: 3,
            fromSelector: false,
            server: null,
            revision: 1,
          },
        ],
      },
    ] as unknown as SuggestedSweepView[]),
  pageSuggestedOps: (_groupId: number, offset: number) =>
    Promise.resolve({
      offset,
      total: 3,
      ops: [1, 2, 3].map((opId) => ({
        opId,
        sourcePath: `/Users/someone/Downloads/setup-${String(opId)}.dmg`,
        newName: null,
        status: 'pending',
        snapshotSize: 1024,
        snapshotModified: 1_780_000_000,
      })),
    } as unknown as SuggestedOpPage),
  liveSuggestedOpFacts: () => Promise.resolve([]),
  onSuggestionsChanged: () => Promise.resolve(() => {}),
}))
vi.mock('#lib/icon-cache.js', async () => {
  const { writable } = await import('svelte/store')
  return { getCachedIcon: () => undefined, iconCacheVersion: writable(0), prefetchIcons: () => Promise.resolve() }
})

import { closeSuggestedOps, openSuggestedOps, suggestedOpsState } from './suggested-ops-trigger.svelte'
import SuggestedOpsDialog from './SuggestedOpsDialog.svelte'

describe('closing the dialog', () => {
  it('tears down with its file rows on screen without throwing', async () => {
    await openSuggestedOps()
    const host = document.createElement('div')
    document.body.appendChild(host)
    const dialog = mount(SuggestedOpsDialog, { target: host })
    flushSync()
    expect(host.querySelectorAll('.op-row').length).toBe(3)

    // The order the app runs it in: the close handler, then the `{#if open}` unmounting.
    closeSuggestedOps()
    await expect(unmount(dialog)).resolves.toBeUndefined()
    expect(suggestedOpsState.open).toBe(false)
  })
})
