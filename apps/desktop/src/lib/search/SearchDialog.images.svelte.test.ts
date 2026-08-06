/**
 * The image-OCR grid searches the volume the user is standing on.
 *
 * The whole point of the `searchVolume` prop in one file: browsing a NAS must query the
 * NAS's media index rather than the local `root`, and an index-relative hit must resolve
 * back to an openable path under that volume's mount root.
 *
 * Shared mount + IPC fixture: `test-search-dialog-harness.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { tick } from 'svelte'
import SearchDialog from './SearchDialog.svelte'
import { setQuery } from './search-state.svelte'
import {
  mediaSearchOcrMock,
  mediaVolumeStateMock,
  mountDialog,
  resetSearchDialogTest,
  unmountAllDialogs,
  useSearchDialog,
} from './test-search-dialog-harness'

vi.mock('$lib/tauri-commands', async () => (await import('./test-search-dialog-harness')).tauriCommandsMock())
vi.mock('../../routes/viewer/media-view', async () => (await import('./test-search-dialog-harness')).mediaViewMock())
vi.mock('$lib/settings', async () => (await import('./test-search-dialog-harness')).settingsMock())
vi.mock('$lib/indexing', async () => (await import('./test-search-dialog-harness')).indexingMock())
vi.mock('$lib/icon-cache', async () => (await import('./test-search-dialog-harness')).iconCacheMock())

useSearchDialog(SearchDialog)

afterEach(unmountAllDialogs)

describe('SearchDialog image-OCR grid targets the active volume', () => {
  beforeEach(async () => {
    // Auto-apply off keeps the filename search out of the way; the grid is query-driven.
    await resetSearchDialogTest({ autoApply: false })
    mediaSearchOcrMock.mockClear()
    mediaVolumeStateMock.mockClear()
  })

  it("searches the focused pane's network volume, resolving hits under its mount root", async () => {
    // The whole point of the feature: browsing the NAS and searching must query the NAS's
    // media index (not the hardcoded local `root`), and the index-relative hit must resolve
    // to an openable OS path under the volume's mount root.
    let navigatedTo: string | null = null
    const { cleanup } = await mountDialog({
      searchVolume: { volumeId: 'smb-naspi', mountRoot: '/Volumes/naspi', isNetwork: true },
      onNavigate: (path) => {
        navigatedTo = path
      },
    })
    vi.useFakeTimers()
    try {
      setQuery('invoice')
      // Fire the grid's 300 ms debounce and let the awaited IPC mocks resolve.
      await vi.advanceTimersByTimeAsync(400)
      await tick()

      // Both the coverage-state read and the OCR search hit the ACTIVE (network) volume id.
      expect(mediaVolumeStateMock).toHaveBeenCalledWith('smb-naspi')
      expect(mediaSearchOcrMock).toHaveBeenCalledWith('smb-naspi', 'invoice', null)
    } finally {
      vi.useRealTimers()
    }

    // The tile opens the mount-root-resolved absolute path, not the index-relative one.
    const tile = document.body.querySelector<HTMLButtonElement>('.ir-tile')
    expect(tile).not.toBeNull()
    tile?.click()
    await tick()
    expect(navigatedTo).toBe('/Volumes/naspi/DCIM/photo.png')

    cleanup()
  })

  it('defaults to the local root volume when no searchVolume prop is passed', async () => {
    // Back-compat: the filename search stays local-index-scoped, and an unspecified
    // image volume must keep the previous local-root behavior (mount root "/").
    const { cleanup } = await mountDialog()
    vi.useFakeTimers()
    try {
      setQuery('invoice')
      await vi.advanceTimersByTimeAsync(400)
      await tick()
      expect(mediaSearchOcrMock).toHaveBeenCalledWith('root', 'invoice', null)
    } finally {
      vi.useRealTimers()
    }
    cleanup()
  })
})
