/**
 * The harness's own guard: `mountFullList` refuses to hand back a list whose
 * rows never arrived.
 *
 * This is the exact shape of the bug it exists to close. A `FullList` over a
 * zero-height surface renders no rows, silently — and every spec written against
 * it then asserts over an empty DOM and passes. The guard turns that into a
 * named failure at the mount, before a single assertion runs.
 */

import { describe, it, expect, vi } from 'vitest'
import { fileEntry, mountFullList } from './test-full-list'

vi.mock('$lib/tauri-commands', async () => (await import('./test-file-list-mocks')).tauriCommandsMock())
vi.mock('$lib/icon-cache', async () => (await import('./test-file-list-mocks')).iconCacheMock())
vi.mock('$lib/indexing/index-state.svelte', async () => (await import('./test-file-list-mocks')).indexStateMock())
vi.mock('$lib/settings/reactive-settings.svelte', async () =>
  (await import('./test-file-list-mocks')).reactiveSettingsMock(),
)
vi.mock('$lib/settings/settings-store', async () => (await import('./test-file-list-mocks')).settingsStoreMock())

describe('mountFullList', () => {
  it('throws when the listing has entries that never reach the screen', async () => {
    await expect(
      mountFullList({ entries: [fileEntry({ name: 'a.txt' })], viewport: { clientHeight: 0 } }),
    ).rejects.toThrow(/timed out waiting for the first entry \(a\.txt\)/)
  })

  it('returns an empty list for an empty listing, rather than waiting for rows', async () => {
    const list = await mountFullList()

    expect(list.rowNames()).toEqual([])
    expect(list.target.querySelector('.empty-folder-message')).toBeTruthy()
  })
})
