/**
 * Fixtures for the rename dialogs (`#lib/file-explorer/rename/`):
 * `extension-change` and `rename-conflict`.
 *
 * Raw copy on purpose: this module is dev-only and sits outside the i18n-enforced
 * areas, so fixture strings never reach the message catalog.
 */

import type { ConflictFileInfo } from '#lib/file-explorer/rename/rename-operations.js'
import { daysAgo, hoursAgo } from './relative-time'

/** Props of `ExtensionChangeDialog.svelte`, minus its callbacks. */
export interface ExtensionChangeFixture {
  oldExtension: string
  newExtension: string
  isDirectory?: boolean
}

/** Props of `RenameConflictDialog.svelte`, minus its callback. */
export interface RenameConflictFixture {
  renamedFile: ConflictFileInfo
  existingFile: ConflictFileInfo
  supportsTrash: boolean
}

/** Keyed by the `extension-change` entry's state ids in `gallery-registry.ts`. */
export const extensionChangeFixtures: Record<string, ExtensionChangeFixture | undefined> = {
  typical: {
    oldExtension: 'txt',
    newExtension: 'zip',
  },
  // Both button labels interpolate the extension, so a long one is where the
  // footer row runs out of width.
  'long-extension': {
    oldExtension: 'sqlite3-journal',
    newExtension: 'photoslibrary',
  },
  'removed-extension': {
    oldExtension: 'txt',
    newExtension: '',
  },
  'removed-package-extension': {
    oldExtension: 'app',
    newExtension: '',
    isDirectory: true,
  },
  'added-extension': {
    oldExtension: '',
    newExtension: 'md',
  },
}

/**
 * Keyed by the `rename-conflict` entry's state ids in `gallery-registry.ts`.
 *
 * Both directions are here because the dialog is a COMPARISON: it tints whichever
 * side is newer and whichever is larger, so a single state leaves half the
 * treatment unreviewed. The third state is the volume with no Trash, where the
 * footer loses its primary button and Enter lands on the red one. The folder
 * states lose the red one: a folder is never deleted for good from here.
 */
export const renameConflictFixtures: Record<string, RenameConflictFixture | undefined> = {
  'newer-and-larger': {
    renamedFile: {
      name: '2026-07-14_test-harbor-sunrise-photo-session_DSC09241_final.arw',
      size: 118_293_504,
      modifiedAt: hoursAgo(3),
      isDirectory: false,
    },
    existingFile: {
      name: '2026-07-14_test-harbor-sunrise-photo-session_DSC09241_final.arw',
      size: 41_238_912,
      modifiedAt: daysAgo(214),
      isDirectory: false,
    },
    supportsTrash: true,
  },
  'older-and-smaller': {
    renamedFile: {
      name: 'notes.md',
      size: 2_048,
      modifiedAt: daysAgo(430),
      isDirectory: false,
    },
    existingFile: {
      name: 'notes.md',
      size: 184_320,
      modifiedAt: hoursAgo(1),
      isDirectory: false,
    },
    supportsTrash: true,
  },
  'no-trash': {
    renamedFile: {
      name: 'MIG008 Odberateľ.2026-03-31.swm.full.01.xml',
      size: 1_204_224,
      modifiedAt: hoursAgo(2),
      isDirectory: false,
    },
    existingFile: {
      name: 'MIG008 Odberateľ.2026-03-31.swm.full.01.xml',
      size: 1_198_080,
      modifiedAt: daysAgo(31),
      isDirectory: false,
    },
    supportsTrash: false,
  },
  // A folder holds the name: no size to compare, and never a permanent delete.
  folder: {
    renamedFile: {
      name: 'Holiday photos',
      size: 4_096_000,
      modifiedAt: hoursAgo(5),
      isDirectory: false,
    },
    existingFile: {
      name: 'Holiday photos',
      size: 0,
      modifiedAt: daysAgo(12),
      isDirectory: true,
    },
    supportsTrash: true,
  },
  'folder-no-trash': {
    renamedFile: {
      name: 'Holiday photos',
      size: 4_096_000,
      modifiedAt: hoursAgo(5),
      isDirectory: false,
    },
    existingFile: {
      name: 'Holiday photos',
      size: 0,
      modifiedAt: daysAgo(12),
      isDirectory: true,
    },
    supportsTrash: false,
  },
}
