/**
 * Turns the Debug window's fixture-directory payload into everything the
 * disk-backed dialogs need to behave for real.
 *
 * Two things a directory path alone can't give them:
 *
 * - `mkdir-confirmation` / `new-file-confirmation` need a LIVE `listingId`. It's a
 *   pane-owned handle (conflict lookup, directory-diff filter, `refreshListing`),
 *   not something a directory produces, so the gallery navigates the focused pane
 *   to the fixture directory and takes that pane's real one. ❌ Never hand them a
 *   made-up id: the conflict check then misbehaves SILENTLY, which is exactly the
 *   "renders broken, wastes the review" failure the gallery exists to prevent.
 * - `delete-confirmation` / `transfer-confirmation` need real ENTRIES. They come
 *   from the same `getFilesAtIndices` call the production trigger path uses, so
 *   the names, sizes, and folder flags on screen are the ones on disk.
 *
 * Navigating the focused pane is a real side effect, and every disk-backed row
 * says so.
 */

import { getFilesAtIndices } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { navigateToDirInPane, resolveLocationOrToast } from '$lib/file-explorer/navigation/navigate-and-select'
import { explorerState } from '$lib/file-explorer/pane/explorer-state.svelte'
import { getActiveTab } from '$lib/file-explorer/tabs/tab-state-manager.svelte'
import type { ExplorerAPI } from '../../routes/(main)/explorer-api'
import type { GalleryDiskFixture } from './gallery-state.svelte'

const log = getAppLogger('dialogGallery')

/** The landmark set `create_dialog_gallery_fixtures` returns, ferried from the Debug window. */
export interface FixtureDirPayload {
  root: string
  destinationDir: string
  existingFolderName: string
  existingFileName: string
  nestedPath: string
}

/**
 * How many top-of-listing entries to fetch. Enough for the "many items" delete
 * and transfer states, few enough to stay one IPC round trip.
 */
const ENTRY_COUNT = 6

/**
 * Navigates the focused pane to the fixture directory and reads back everything
 * the disk-backed dialogs need. Returns `null` when the pane can't get there, so
 * the caller opens nothing rather than a half-real dialog.
 */
export async function resolveDiskFixture(
  explorer: ExplorerAPI | undefined,
  fixtures: FixtureDirPayload,
): Promise<GalleryDiskFixture | null> {
  if (!explorer) {
    log.warn('Dialog gallery: no explorer yet, skipping the disk-backed preview')
    return null
  }

  const location = await resolveLocationOrToast(fixtures.root)
  if (!location) return null

  const paneSide = explorer.getFocusedPane()
  await navigateToDirInPane(explorer, paneSide, location)

  const listingId = explorer.getPaneListingId(paneSide)
  if (!listingId) {
    log.warn('Dialog gallery: the {pane} pane has no listing for {root}', { pane: paneSide, root: fixtures.root })
    return null
  }

  const showHiddenFiles = explorerState.getShowHiddenFiles()
  const tab = getActiveTab(explorerState.getTabMgr(paneSide))

  // Backend indices: 0 is the first real entry, so the synthetic `..` row never
  // reaches a fixture. A short listing simply returns fewer entries.
  const indices = Array.from({ length: ENTRY_COUNT }, (_, index) => index)
  const entries = await getFilesAtIndices(listingId, indices, showHiddenFiles)

  return {
    ...fixtures,
    paneSide,
    listingId,
    volumeId: tab.volumeId,
    showHiddenFiles,
    sortColumn: tab.sortBy,
    sortOrder: tab.sortOrder,
    entries,
  }
}
