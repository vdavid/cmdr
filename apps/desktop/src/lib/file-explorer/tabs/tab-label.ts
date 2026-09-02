import { getFolderName } from '$lib/file-operations/transfer/transfer-dialog-utils'
import { getDeviceDisplayPath, isDeviceScheme } from '$lib/adb/adb-path-utils'

/**
 * Derives the user-facing label for a tab from its path.
 *
 * Normally the basename is the right thing to show ("Documents" for
 * `/Users/john/Documents`, "/" for the local filesystem root). But an MTP path
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
 */
export function deriveTabLabel(path: string): string {
  if (isDeviceScheme(path)) {
    // `getDeviceDisplayPath` returns "/" at the storage or device root and
    // `/DCIM/Camera` for a subfolder; its basename is the tab label.
    return getFolderName(getDeviceDisplayPath(path))
  }
  return getFolderName(path)
}
