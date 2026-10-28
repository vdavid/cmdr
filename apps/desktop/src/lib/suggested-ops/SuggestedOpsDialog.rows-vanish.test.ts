/**
 * A drawn row whose op vanishes from under it, over the REAL state slice.
 *
 * Regression anchor: a row's cells re-read their op on the way out (Ark's checkbox reads its
 * `checked` prop while it tears down), so a group whose review went in the same flush as its
 * rows threw `undefined is not an object (evaluating 'op.status')` and took the main window
 * down to its crash panel. Debug > Soft dialogs reached it by opening one AI suggestions state
 * over another: the seed swaps the whole store in one step, so the first state's groups leave
 * with their reviews. The second test is the one that reproduces it here; the rest pin the
 * neighbouring swaps (same group id, a reload mid-page, the gallery's own seed). The approve and
 * close paths' twins: `SuggestedOpsDialog.approve.test.ts`, `SuggestedOpsDialog.close.test.ts`.
 */

import { describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick, unmount } from 'svelte'
import { SvelteMap } from 'svelte/reactivity'
import type { SuggestedOpPage, SuggestedSweepView } from '#lib/tauri-commands/index.js'

const group = {
  groupId: 7,
  sweepId: 1,
  verb: 'move',
  status: 'pending',
  displayName: 'Installers',
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
}

/** Lets a test hold the next page read open, so the rows sit without their window meanwhile. */
const pages = { hold: null as Promise<void> | null }

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<object>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  listSuggestedOps: () =>
    Promise.resolve([
      { sweepId: 1, createdAt: 1_780_000_000, rationale: null, groups: [group] },
    ] as unknown as SuggestedSweepView[]),
  pageSuggestedOps: async (groupId: number, offset: number) => {
    if (pages.hold) await pages.hold
    return {
      offset,
      total: 3,
      ops: [1, 2, 3].map((n) => ({
        opId: groupId * 10 + n,
        sourcePath: `/Users/someone/Downloads/setup-${String(n)}.dmg`,
        newName: null,
        status: 'pending',
        snapshotSize: 1024,
        snapshotModified: 1_780_000_000,
      })),
    } as unknown as SuggestedOpPage
  },
  liveSuggestedOpFacts: () => Promise.resolve({ kind: 'facts', facts: [] }),
  onSuggestionsChanged: () => Promise.resolve(() => {}),
}))
vi.mock('#lib/icon-cache.js', async () => {
  const { writable } = await import('svelte/store')
  return { getCachedIcon: () => undefined, iconCacheVersion: writable(0), prefetchIcons: () => Promise.resolve() }
})

import {
  closeSuggestedOps,
  newGroupReview,
  openSuggestedOps,
  reloadGroup,
  suggestedOpsState,
  type GroupReview,
} from './suggested-ops-trigger.svelte'
import SuggestedOpsDialog from './SuggestedOpsDialog.svelte'
import { buildStoreSeed } from '#lib/dialog-gallery/fixtures/store-seeded.js'

async function mountWithRows(): Promise<{ host: HTMLElement; dialog: ReturnType<typeof mount> }> {
  await openSuggestedOps()
  const host = document.createElement('div')
  document.body.appendChild(host)
  const dialog = mount(SuggestedOpsDialog, { target: host })
  flushSync()
  expect(host.querySelectorAll('.op-row.is-data').length).toBe(3)
  return { host, dialog }
}

describe('a drawn row whose op vanishes', () => {
  it('survives the reviews being swapped for ones whose windows are not held (a second gallery open)', async () => {
    const { host, dialog } = await mountWithRows()

    // What a second Debug > Soft dialogs open does in one go: a fresh reviews map, same group id.
    // The throw lands in Svelte's flush, outside this test's promise chain: Vitest reports it as an
    // unhandled error, which fails the run.
    suggestedOpsState.reviews = new SvelteMap<number, GroupReview>([[7, newGroupReview()]])
    flushSync()
    await tick()
    expect(host.querySelectorAll('.op-row.is-data').length).toBe(0)

    closeSuggestedOps()
    await unmount(dialog)
  })

  it('survives the sweeps and reviews being swapped for other groups in one go (a second gallery open)', async () => {
    const { host, dialog } = await mountWithRows()

    // Debug > Soft dialogs > AI suggestions, first state then second: the seed puts the store
    // back and applies the next patch in one synchronous step, so group 7 leaves the list in the
    // same flush its review does.
    const other = { ...group, groupId: 9, displayName: 'Other' }
    suggestedOpsState.sweeps = [
      { sweepId: 2, createdAt: 1_780_000_000, rationale: null, groups: [other] },
    ] as unknown as SuggestedSweepView[]
    suggestedOpsState.reviews.delete(7)
    suggestedOpsState.reviews.set(9, newGroupReview())
    flushSync()
    await tick()
    expect(host.textContent).toContain('Other')

    closeSuggestedOps()
    await unmount(dialog)
  })

  it('survives Debug > Soft dialogs opening one seeded state over another, exactly as the gallery does', async () => {
    const first = buildStoreSeed('suggested-ops', 'downloads-cleanup')
    const second = buildStoreSeed('suggested-ops', 'hard-cases')
    if (!first || !second) throw new Error('the gallery fixtures exist')
    const undoFirst = first.apply()
    const host = document.createElement('div')
    document.body.appendChild(host)
    const dialog = mount(SuggestedOpsDialog, { target: host })
    flushSync()
    expect(host.textContent).toContain('Installers you’ve already run')

    // The gallery's `$effect` cleanup puts the store back, then the next run seeds the second
    // state, in one synchronous step.
    undoFirst()
    const undoSecond = second.apply()
    flushSync()
    await tick()
    expect(host.textContent).toContain('Build caches')

    undoSecond()
    closeSuggestedOps()
    await unmount(dialog)
  })

  it('survives "Show the new version" dropping the held window before the new page lands', async () => {
    const { host, dialog } = await mountWithRows()

    let release = (): void => {}
    pages.hold = new Promise<void>((resolve) => (release = resolve))
    const reloading = reloadGroup(7)
    flushSync()
    await tick()
    release()
    pages.hold = null
    await reloading
    flushSync()
    expect(host.querySelectorAll('.op-row.is-data').length).toBe(3)

    closeSuggestedOps()
    await unmount(dialog)
  })
})
