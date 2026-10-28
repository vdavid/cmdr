/**
 * Approving a file group with its rows on screen, over the REAL state slice.
 *
 * Regression anchor: the re-read after an approval dropped the answered group's review in the
 * same tick its rows unmounted. The rows' cells re-read their op on the way out (Ark's checkbox
 * reads its `checked` prop), found it gone, and threw `undefined is not an object (evaluating
 * 't.status')`, which took the whole main window down to its crash panel. The E2E suite caught
 * it as every spec after `suggested-ops-named-move` failing on a wedged app. The close path's
 * twin is `SuggestedOpsDialog.close.test.ts`.
 */

import { describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick, unmount } from 'svelte'
import type { SuggestedOpPage, SuggestedSweepView } from '#lib/tauri-commands/index.js'

const groupNamed = (groupId: number, displayName: string) => ({
  groupId,
  sweepId: 1,
  verb: 'move',
  status: 'pending',
  displayName,
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
})

/** What the store holds: both groups until the first one is approved. */
const waiting = { groupIds: [7, 8] }

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
        groups: waiting.groupIds.map((groupId) => groupNamed(groupId, `Group ${String(groupId)}`)),
      },
    ] as unknown as SuggestedSweepView[]),
  pageSuggestedOps: (groupId: number, offset: number) =>
    Promise.resolve({
      offset,
      total: 3,
      ops: [1, 2, 3].map((n) => ({
        opId: groupId * 10 + n,
        sourcePath: `/Users/someone/Downloads/setup-${String(groupId)}-${String(n)}.dmg`,
        newName: null,
        status: 'pending',
        snapshotSize: 1024,
        snapshotModified: 1_780_000_000,
      })),
    } as unknown as SuggestedOpPage),
  liveSuggestedOpFacts: () => Promise.resolve([]),
  onSuggestionsChanged: () => Promise.resolve(() => {}),
  approveSuggestedGroup: () => {
    waiting.groupIds = [8]
    return Promise.resolve({ kind: 'started' })
  },
}))
vi.mock('#lib/icon-cache.js', async () => {
  const { writable } = await import('svelte/store')
  return { getCachedIcon: () => undefined, iconCacheVersion: writable(0), prefetchIcons: () => Promise.resolve() }
})

import { approveGroup, closeSuggestedOps, openSuggestedOps, reviewOf } from './suggested-ops-trigger.svelte'
import SuggestedOpsDialog from './SuggestedOpsDialog.svelte'

describe('approving a group', () => {
  it('takes its rows off screen without throwing, and the group left waiting keeps its own', async () => {
    await openSuggestedOps()
    const host = document.createElement('div')
    document.body.appendChild(host)
    const dialog = mount(SuggestedOpsDialog, { target: host })
    flushSync()
    expect(host.querySelectorAll('.op-row').length).toBe(6)

    // The throw lands in Svelte's flush, outside this test's promise chain: Vitest reports it as
    // an unhandled error, which fails the run.
    await approveGroup(7)
    flushSync()
    await tick()

    expect(host.querySelectorAll('.op-row').length).toBe(3)
    // The answered group's review goes once nothing renders it.
    expect(reviewOf(7)).toBeUndefined()
    expect(reviewOf(8)).toBeDefined()

    closeSuggestedOps()
    await unmount(dialog)
  })
})
