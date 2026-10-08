/**
 * Fixtures for `transfer-error` (`$lib/file-operations/transfer/TransferErrorDialog.svelte`).
 *
 * The dialog renders ENTIRELY from the typed `WriteOperationError` plus the
 * operation type: title, explanation, suggestion, icon, container tint, and the
 * Retry button all derive from the variant (`transfer-error-messages.ts`). So a
 * faithful preview is a real typed error and nothing else, and every variant
 * earns its own state.
 *
 * `operationType` is picked per variant to match where the error can actually
 * happen (`trash_not_supported` on a trash, `delete_pending` on a delete), since
 * most of the copy has a per-operation phrasing.
 *
 * Raw copy on purpose: this module is dev-only and sits outside the i18n-enforced
 * areas, so fixture strings never reach the message catalog.
 */

import type { TransferOperationType, WriteOperationError } from '$lib/file-explorer/types'
import type { ProgressAtStop } from '$lib/tauri-commands'

/** Props of `TransferErrorDialog.svelte`, minus its callbacks. */
export interface TransferErrorFixture {
  operationType: TransferOperationType
  error: WriteOperationError
  /** How far the operation got, for the variants whose copy says so. */
  progressAtStop?: ProgressAtStop
}

const LONG_PATH =
  '/Volumes/Naspolya/media/photos/2026/07-summer-archive/raw-originals/Sony-A7RV/2026-07-14_stockholm-archipelago-sunrise-session/DSC09241_edited_final_v3_reallyfinal.arw'

/**
 * One fixture per `WriteOperationError` variant. A `Record` keyed by every
 * variant makes adding a variant a compile error here, which is the same guard
 * `errorDisplayMetaMap` uses, and it's what keeps "the gallery covers every
 * error the dialog can show" true rather than aspirational.
 *
 * State ids are the variant tags verbatim, so a row's button maps to the union
 * member with no lookup table in between.
 */
const perVariant: Record<WriteOperationError['type'], TransferErrorFixture> = {
  source_not_found: {
    operationType: 'copy',
    error: { type: 'source_not_found', path: LONG_PATH },
  },
  destination_not_found: {
    operationType: 'copy',
    // A share subfolder, the shape this actually shows up in: the destination
    // is on a NAS that stopped being able to address the folder mid-transfer.
    error: { type: 'destination_not_found', path: '/Volumes/Naspolya/media/photos/2026' },
  },
  destination_not_a_folder: {
    operationType: 'copy',
    // The path is the FILE in the way, a level above the folder the copy was
    // told to create (`…/photos/2026/trip`).
    error: { type: 'destination_not_a_folder', path: '/Volumes/Naspolya/media/photos/2026' },
  },
  source_not_connected: {
    operationType: 'delete',
    // A phone the switcher lists that nobody has opened in a pane yet.
    error: { type: 'source_not_connected', path: 'adb://46061FDAS000A4/sdcard/DCIM/Camera/PXL_20260714_052311.jpg' },
  },
  destination_not_connected: {
    operationType: 'copy',
    // A saved server nobody has connected this session.
    error: { type: 'destination_not_connected', path: 'sftp://david@naspolya.local:22/share/photos/2026' },
  },
  source_no_longer_connected: {
    operationType: 'copy',
    // A phone unplugged while its search results were still on screen.
    error: { type: 'source_no_longer_connected', path: '/sdcard/DCIM/Camera/PXL_20260714_052311.jpg' },
  },
  destination_exists: {
    operationType: 'move',
    error: { type: 'destination_exists', path: '/Users/david/Documents/invoices/2026-Q2-summary.numbers' },
  },
  permission_denied: {
    operationType: 'delete',
    error: {
      type: 'permission_denied',
      path: '/Library/Application Support/com.apple.TCC/TCC.db',
      // Multi-line raw detail: the details block either scrolls it or blows out
      // the dialog, and a one-liner would never show which.
      message: 'os error 1: Operation not permitted\nsandbox: deny(1) file-write-unlink /Library/Application Support',
      errno: null,
      refusal: 'unclassified',
      refusedFolder: null,
      side: null,
    },
  },
  insufficient_space: {
    operationType: 'copy',
    error: {
      type: 'insufficient_space',
      required: 214_748_364_800,
      available: 3_221_225_472,
      volumeName: 'Naspolya media (SMB)',
    },
  },
  destination_full: {
    operationType: 'copy',
    // A USB stick that filled up partway: the write refused, nothing measured.
    error: { type: 'destination_full', path: '/Volumes/USB-STICK/photos/2026/DSC09241.arw' },
  },
  destination_inside_source: {
    operationType: 'move',
    error: {
      type: 'destination_inside_source',
      source: '/Users/david/Pictures/Photo Library',
      destination: '/Users/david/Pictures/Photo Library/2026/backup-of-everything',
    },
  },
  duplicate_source_names: {
    operationType: 'move',
    error: {
      type: 'duplicate_source_names',
      name: 'invoices',
      first: '/Users/david/Documents/2025/invoices',
      second: '/Users/david/Documents/2026/invoices',
    },
  },
  archive_entry_name_refused: {
    operationType: 'compress',
    error: {
      type: 'archive_entry_name_refused',
      entry: 'Reports/..\\notes.txt',
      reason: 'parentTraversal',
    },
  },
  archive_entry_names_collide: {
    operationType: 'compress',
    error: {
      type: 'archive_entry_names_collide',
      entry: 'Reports/a\\b.txt',
      other: 'Reports/a/b.txt',
      archivePath: 'Reports/a/b.txt',
    },
  },
  symlink_loop: {
    operationType: 'copy',
    error: { type: 'symlink_loop', path: '/Users/david/dev/node_modules/.pnpm/self/node_modules/self' },
  },
  cancelled: {
    operationType: 'copy',
    error: { type: 'cancelled', message: 'Cancelled after 1,284 of 12,900 files' },
  },
  // The drive files were being copied TO, with the progress the backend read
  // before the operation unregistered: this is the sentence someone reads when
  // they've just pulled a stick mid-copy and want to know what they have.
  device_disconnected: {
    operationType: 'copy',
    error: {
      type: 'device_disconnected',
      path: '/Volumes/Fältkamera/DCIM/104MSDCF',
      side: {
        role: 'destination',
        volumeId: 'vol-faltkamera',
        volumeName: 'Fältkamera',
        counterpartName: 'Macintosh HD',
      },
    },
    progressAtStop: {
      filesDone: 1284,
      filesTotal: 12900,
      bytesDone: 4_920_000_000,
      bytesTotal: 51_000_000_000,
      sourcesRemoved: null,
      sourcesLeft: null,
    },
  },
  move_not_confirmed: {
    operationType: 'move',
    error: {
      type: 'move_not_confirmed',
      path: '/Volumes/Fältkamera/DCIM',
      errno: 5,
      volumeName: 'Fältkamera',
    },
  },
  read_only_device: {
    operationType: 'move',
    error: {
      type: 'read_only_device',
      path: '/Volumes/Cmdr 0.9.4/Cmdr.app',
      deviceName: 'Cmdr 0.9.4 (disk image)',
      side: 'destination',
    },
  },
  destination_not_writable: {
    operationType: 'copy',
    error: { type: 'destination_not_writable', path: 'adb://46061FDAS000A4', reason: 'unexplained' },
  },
  file_locked: {
    operationType: 'delete',
    error: { type: 'file_locked', path: '/Users/david/Documents/Rymdskottkärra/bokföring-2025.numbers' },
  },
  trash_not_supported: {
    operationType: 'trash',
    error: { type: 'trash_not_supported', path: '/Volumes/naspi/papers/finances/2026/kvitton' },
  },
  connection_interrupted: {
    operationType: 'copy',
    error: { type: 'connection_interrupted', path: 'smb://naspolya.local/media/photos/2026' },
  },
  read_error: {
    operationType: 'copy',
    error: {
      type: 'read_error',
      path: '/Volumes/Fältkamera/DCIM/104MSDCF/DSC09241.ARW',
      message: 'os error 5: Input/output error (block 2,398,112)',
    },
  },
  write_error: {
    operationType: 'copy',
    error: {
      type: 'write_error',
      path: '/Volumes/naspi/media/photos/2026/DSC09241.ARW',
      message: 'os error 28: No space left on device',
    },
  },
  name_too_long: {
    operationType: 'copy',
    error: { type: 'name_too_long', path: `${LONG_PATH}.duplicate-of-the-duplicate-with-a-very-long-suffix` },
  },
  invalid_name: {
    operationType: 'move',
    error: {
      type: 'invalid_name',
      path: '/Volumes/USB-STICK (FAT32)/notes: draft?.md',
      message: 'FAT32 file names can’t contain : or ?',
    },
  },
  delete_pending: {
    operationType: 'delete',
    error: { type: 'delete_pending', path: '/Users/david/Downloads/ubuntu-26.04-desktop-amd64.iso' },
  },
  // A copy off an S3 bucket meeting an object in Glacier.
  source_in_cold_storage: {
    operationType: 'copy',
    error: {
      type: 'source_in_cold_storage',
      path: 's3://AKIAIOSFODNN7EXAMPLE@s3.eu-west-1.amazonaws.com:443/family-photos/2019/holiday-raw.tar',
    },
  },
  // A move within an S3 account whose source another app replaced mid-copy.
  source_changed: {
    operationType: 'move',
    error: {
      type: 'source_changed',
      path: 's3://AKIAIOSFODNN7EXAMPLE@nbg1.your-objectstorage.com:443/team-docs/2026/budget.xlsx',
    },
  },
  // The many-files branch: the body copy counts them and the details block lists
  // every one, so this is where the dialog gets tall.
  files_too_large_for_filesystem: {
    operationType: 'copy',
    error: {
      type: 'files_too_large_for_filesystem',
      filesystem: 'fat32',
      maxSize: 4_294_967_295,
      files: [
        { name: '2026-07-14_stockholm-archipelago-sunrise-session.braw', size: 92_341_338_112 },
        { name: 'family-videos-2011-2026-master.mov', size: 41_231_223_808 },
        { name: 'ubuntu-26.04-desktop-amd64.iso', size: 6_442_450_944 },
      ],
      totalCount: 3,
    },
  },
  // The new file is written and complete; it just couldn't take the name, and the
  // one it was replacing is already gone. Both paths matter to the reader, so the
  // fixture uses names that differ only by the recovered suffix.
  new_data_kept_at: {
    operationType: 'copy',
    error: {
      type: 'new_data_kept_at',
      path: '/Volumes/naspi/papers/finances/2026-tax-return.pdf',
      keptAt: '/Volumes/naspi/papers/finances/2026-tax-return (recovered).pdf',
      message: 'os error 60: Operation timed out',
    },
  },
  // The copy stopped partway with a folder standing where one of the user's files
  // was. The fixture keeps the folder's name and the recovered name side by side,
  // which is the pair the reader has to make sense of.
  originals_kept_aside: {
    operationType: 'copy',
    error: {
      type: 'originals_kept_aside',
      cause: { type: 'source_not_found', path: '/Users/david/projects/notes/2026-07-24.md' },
      recovered: [
        {
          path: '/Volumes/naspi/papers/finances/2026-tax-return',
          keptAt: '/Volumes/naspi/papers/finances/2026-tax-return (recovered)',
        },
      ],
    },
  },
  // cmdr-reports#17's shape: the copy landed on the NAS and a Finder-locked original
  // refused to go, so the item is in both places. The cause's advice (uncheck Locked)
  // rides in the suggestion.
  source_not_removed: {
    operationType: 'move',
    error: {
      type: 'source_not_removed',
      path: '/Users/david/Pictures/gate-measurements.jpg',
      landedAt: '/Volumes/naspi/projects-archive/2023/gate-measurements.jpg',
      cause: {
        type: 'permission_denied',
        path: '/Users/david/Pictures/gate-measurements.jpg',
        message: 'Operation not permitted (os error 1)',
        errno: 1,
        refusal: 'systemProtected',
        refusedFolder: null,
        side: 'source',
      },
    },
  },
  // The shape a real report arrived as: several screenshots in a cloud folder that
  // macOS refused on a permission code. The Full Disk Access line only joins the
  // suggestion when this Mac is actually missing the grant, so the gallery shows the
  // base wording.
  trash_refused: {
    operationType: 'trash',
    error: {
      type: 'trash_refused',
      itemCount: 7,
      reason: 'notPermitted',
      message:
        '$HOME/Library/CloudStorage/Dropbox/Shots/Screenshot.jpeg: “Screenshot.jpeg” couldn’t be moved to the trash because you don’t have permission to access it.',
    },
  },
  io_error: {
    operationType: 'copy',
    error: {
      type: 'io_error',
      path: '/Volumes/naspi/media/photos/2026',
      message: 'os error 60: Operation timed out',
    },
  },
  archive_needs_password: {
    operationType: 'copy',
    error: {
      type: 'archive_needs_password',
      path: '/Users/david/Downloads/tax-returns-2019-2025.zip',
      wrongAttempt: false,
    },
  },
}

/**
 * States where one variant renders a meaningfully different layout. Only
 * `files_too_large_for_filesystem` does: its message factory has a distinct
 * single-file branch that names the file inline instead of counting.
 */
const extraStates: Record<string, TransferErrorFixture> = {
  'files_too_large_for_filesystem-single': {
    operationType: 'copy',
    error: {
      type: 'files_too_large_for_filesystem',
      filesystem: 'fat32',
      maxSize: 4_294_967_295,
      files: [{ name: '2026-07-14_stockholm-archipelago-sunrise-session.braw', size: 92_341_338_112 }],
      totalCount: 1,
    },
  },
}

/**
 * Keyed by the `transfer-error` entry's state ids in `gallery-registry.ts`.
 * Values are optional so a lookup by an id that drifted out of the registry is
 * detectable rather than silently typed as present.
 */
export const transferErrorFixtures: Record<string, TransferErrorFixture | undefined> = {
  ...perVariant,
  ...extraStates,
}
