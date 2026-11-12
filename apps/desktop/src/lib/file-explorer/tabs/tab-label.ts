import { getFolderName } from '#lib/file-operations/transfer/transfer-dialog-utils.js'
import { getDeviceDisplayPath, isDeviceScheme } from '#lib/adb/adb-path-utils.js'
import { NETWORK_VOLUME_PATH } from '../pane/navigate-refusals'
import { tString } from '#lib/intl/messages.svelte.js'

/**
 * Derives the user-facing label for a tab from its path.
 *
 * Normally the basename is the right thing to show ("Documents" for
 * `/Users/test_user_14/Documents`, "/" for the local filesystem root). But an MTP path
 * (`mtp://{deviceId}/{storageId}/inner/path`) puts the raw storage id as the
 * last segment at the storage root, which surfaced as the tab title "65537"
 * (0x10001 = Internal Storage). For MTP paths we derive the label from the
 * inner (within-storage) path instead: "/" at the storage root, the inner
 * folder basename below it — matching what the breadcrumb already shows.
 *
 * An ADB path (`adb://{serial}/inner/path`) gets the same treatment: "/" at the
 * device root, the inner folder basename below it.
 *
 * We special-case ONLY the two device schemes. Every other path (including mounted
 * volume roots like `/Volumes/USB`, which keep their basename "USB") flows
 * through `getFolderName` unchanged.
 *
 * The one exception is a volume's root whose `rootLabel` names it (an SMB
 * share, minted in Rust): a share mounted at a disambiguated path
 * (`/Volumes/public-1`) is still called `public`, the name the header shows it by.
 */
export function deriveTabLabel(path: string, volume?: { path: string; rootLabel?: string | null }): string {
  // The Servers view has no folder: its path is the `smb://` sentinel, whose
  // "basename" would read "/". It's called what the switcher calls it.
  if (path === NETWORK_VOLUME_PATH) return tString('fileExplorer.navigation.networkVolume')
  if (volume?.rootLabel && path === volume.path) return volume.rootLabel
  if (isDeviceScheme(path)) {
    // `getDeviceDisplayPath` returns "/" at the storage or device root and
    // `/DCIM/Camera` for a subfolder; its basename is the tab label.
    return getFolderName(getDeviceDisplayPath(path))
  }
  return getFolderName(path)
}
