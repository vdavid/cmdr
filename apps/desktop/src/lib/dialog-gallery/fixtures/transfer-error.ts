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

/** Props of `TransferErrorDialog.svelte`, minus its callbacks. */
export interface TransferErrorFixture {
  operationType: TransferOperationType
  error: WriteOperationError
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
  same_location: {
    operationType: 'move',
    error: { type: 'same_location', path: '/Users/david/projects-git/vdavid/cmdr' },
  },
  destination_inside_source: {
    operationType: 'move',
    error: {
      type: 'destination_inside_source',
      source: '/Users/david/Pictures/Photo Library',
      destination: '/Users/david/Pictures/Photo Library/2026/backup-of-everything',
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
  device_disconnected: {
    operationType: 'copy',
    error: { type: 'device_disconnected', path: '/Volumes/Fältkamera/DCIM/104MSDCF' },
  },
  read_only_device: {
    operationType: 'move',
    error: {
      type: 'read_only_device',
      path: '/Volumes/Cmdr 0.9.4/Cmdr.app',
      deviceName: 'Cmdr 0.9.4 (disk image)',
    },
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
