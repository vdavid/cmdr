/**
 * The per-folder hourglass in the Size column: which rendered rows say "this
 * number is about to move".
 *
 * The answer is the ground a walker is on right now (`getWalkedGround` +
 * `isPathAffectedByWalk`), never "this volume is scanning". Two properties are
 * easy to lose and neither shows up in the predicate's own unit tests, because
 * both are about the wiring between the pane and the predicate:
 *
 * - The test runs BOTH WAYS. The roll-up repairs the ancestor chain, so a walk
 *   below a row moves that row's size too, and a downward-only test leaves every
 *   folder above the walked ground looking settled while its number changes.
 * - The ground is read PER VOLUME, so a walk on one drive can't light up rows in
 *   a pane showing another.
 *
 * `walked-ground.test.ts` covers the predicate itself; this covers what the rows
 * actually render. The mocks come from `test-file-list-mocks.ts`, the measured
 * surface from `test-full-list.ts`.
 */

import { describe, it, expect, vi } from 'vitest'
import { dirEntry, fileEntry, mountFullList } from './test-full-list'

vi.mock('$lib/tauri-commands', async () => (await import('./test-file-list-mocks')).tauriCommandsMock())
vi.mock('$lib/icon-cache', async () => (await import('./test-file-list-mocks')).iconCacheMock())
vi.mock('$lib/settings/reactive-settings.svelte', async () =>
  (await import('./test-file-list-mocks')).reactiveSettingsMock(),
)
vi.mock('$lib/settings/settings-store', async () => (await import('./test-file-list-mocks')).settingsStoreMock())

// The walked ground is per volume, so the mock answers per volume too: `root` has
// a walker on one whole folder and deep inside another; `usb` has none.
vi.mock('$lib/indexing/index-state.svelte', async () =>
  (await import('./test-file-list-mocks')).indexStateMock({
    getWalkedGround: (volumeId: string) =>
      volumeId === 'root' ? ['/root/downloads', '/root/projects/cmdr/target'] : [],
  }),
)

/**
 * One listing at `/root`: the walked folder itself, a near-miss sharing its name
 * prefix, an unrelated folder, a folder ABOVE walked ground, and a plain file.
 */
const ENTRIES = [
  dirEntry({ name: 'downloads' }),
  dirEntry({ name: 'downloads-old' }),
  dirEntry({ name: 'music' }),
  dirEntry({ name: 'projects' }),
  fileEntry({ name: 'notes.txt' }),
]

describe('FullList per-folder walk hourglass', () => {
  it('marks the walked folder and the folder above the walked ground, and nothing else', async () => {
    const list = await mountFullList({ entries: ENTRIES })

    // Every row is on screen: without this the negative half below would pass
    // against an empty list.
    expect(list.rowNames()).toEqual(['downloads', 'downloads-old', 'music', 'projects', 'notes.txt'])

    expect(list.hourglassRowNames()).toEqual([
      // The walker is on this folder itself.
      'downloads',
      // The walker is deep inside `/root/projects/cmdr/target`; the roll-up will
      // move this ancestor's number, so it reads as in flux too.
      'projects',
    ])
  })

  it('leaves every row settled when nothing on this pane’s volume is under a walker', async () => {
    const list = await mountFullList({ entries: ENTRIES, props: { volumeId: 'usb' } })

    expect(list.rowNames()).toHaveLength(5)
    expect(list.hourglassRowNames()).toEqual([])
  })
})
