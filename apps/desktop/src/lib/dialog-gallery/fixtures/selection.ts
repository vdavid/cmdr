/**
 * Fixtures for `selection-add` / `selection-remove`
 * (`$lib/selection-dialog/SelectionDialog.svelte`).
 *
 * The dialog filters a SNAPSHOT of the focused pane's entries taken at open time
 * and passed in as a prop, so a fixture list is exactly what production hands it:
 * no pane, no listing, no IPC involved in the results table.
 *
 * Icon ids follow the real backend convention (`dir`, `ext:<ext>`, `file`), so
 * the rows pull real icons out of the live icon cache instead of rendering blank.
 *
 * Raw copy on purpose: this module is dev-only and sits outside the i18n-enforced
 * areas, so fixture strings never reach the message catalog.
 */

import type { FileEntry } from '$lib/file-explorer/types'
import { daysAgo, hoursAgo } from './relative-time'

const FIXTURE_DIR = '/Volumes/Naspolya/media/photos/2026/07-summer-archive'

/** Builds one entry, filling in the fields the results table doesn't vary. */
function entry(name: string, options: Partial<FileEntry> = {}): FileEntry {
  const extension = name.includes('.') ? name.slice(name.lastIndexOf('.') + 1).toLowerCase() : ''
  return {
    name,
    path: `${FIXTURE_DIR}/${name}`,
    isDirectory: false,
    isSymlink: false,
    size: 4_096,
    modifiedAt: daysAgo(30),
    permissions: 0o644,
    owner: 'david',
    group: 'staff',
    iconId: extension ? `ext:${extension}` : 'file',
    extendedMetadataLoaded: false,
    ...options,
  }
}

/** Builds one directory entry. */
function directory(name: string, options: Partial<FileEntry> = {}): FileEntry {
  return entry(name, { isDirectory: true, iconId: 'dir', size: undefined, permissions: 0o755, ...options })
}

/**
 * A folder with the mix that actually stresses the results table: directories
 * first, a name long enough to need truncation, non-ASCII names, a hidden file, a
 * symlink, a multi-gigabyte file next to a 400-byte one, and enough same-extension
 * files that a `*.arw` filter matches a meaningful subset.
 */
const MIXED_FOLDER: FileEntry[] = [
  directory('raw-originals', { modifiedAt: hoursAgo(2) }),
  directory('exported-jpeg-web-resolution-2048px-longest-edge', { modifiedAt: daysAgo(4) }),
  directory('Färdiga bilder till tryck'),
  directory('.thumbnails', { modifiedAt: daysAgo(120) }),
  entry('2026-07-14_stockholm-archipelago-sunrise-session_DSC09241_edited_final_v3_reallyfinal.arw', {
    size: 118_293_504,
    modifiedAt: hoursAgo(3),
  }),
  entry('DSC09242.arw', { size: 116_802_048, modifiedAt: hoursAgo(3) }),
  entry('DSC09243.arw', { size: 119_734_272, modifiedAt: hoursAgo(3) }),
  entry('DSC09244.arw', { size: 117_440_512, modifiedAt: hoursAgo(3) }),
  entry('DSC09245.arw', { size: 121_634_816, modifiedAt: hoursAgo(2) }),
  entry('DSC09241.jpg', { size: 8_912_896, modifiedAt: hoursAgo(1) }),
  entry('DSC09242.jpg', { size: 8_388_608, modifiedAt: hoursAgo(1) }),
  entry('midsommar-timelapse-master.braw', { size: 92_341_338_112, modifiedAt: daysAgo(41) }),
  entry('familjevideo-2026-sommar.mov', { size: 4_294_967_296, modifiedAt: daysAgo(12) }),
  entry('kontaktkarta.pdf', { size: 1_048_576, modifiedAt: daysAgo(88) }),
  entry('shot-list.md', { size: 4_212, modifiedAt: daysAgo(2) }),
  entry('README.txt', { size: 412, modifiedAt: daysAgo(365) }),
  entry('.DS_Store', { size: 6_148, modifiedAt: daysAgo(1) }),
  entry('latest-export.jpg', { isSymlink: true, size: 0, modifiedAt: daysAgo(4) }),
  entry('轉存清單.csv', { size: 22_016, modifiedAt: daysAgo(7) }),
  entry('backup.tar.gz', { size: 2_684_354_560, modifiedAt: daysAgo(200) }),
]

/** Props of `SelectionDialog.svelte` the fixture owns; `mode` comes from the dialog id. */
export interface SelectionFixture {
  entries: FileEntry[]
  cursorIndex: number
  /** True renders the search-results-snapshot banner. */
  isSnapshotPane: boolean
}

/** Keyed by the `selection-add` entry's state ids in `gallery-registry.ts`. */
export const selectionAddFixtures: Record<string, SelectionFixture | undefined> = {
  'mixed-folder': { entries: MIXED_FOLDER, cursorIndex: 4, isSnapshotPane: false },
  // The snapshot banner only appears when the focused pane is a
  // `search-results://` view, which is otherwise a multi-step thing to stage.
  'snapshot-pane': { entries: MIXED_FOLDER, cursorIndex: 0, isSnapshotPane: true },
  'empty-folder': { entries: [], cursorIndex: 0, isSnapshotPane: false },
}

/** Keyed by the `selection-remove` entry's state ids in `gallery-registry.ts`. */
export const selectionRemoveFixtures: Record<string, SelectionFixture | undefined> = {
  'mixed-folder': { entries: MIXED_FOLDER, cursorIndex: 4, isSnapshotPane: false },
}
