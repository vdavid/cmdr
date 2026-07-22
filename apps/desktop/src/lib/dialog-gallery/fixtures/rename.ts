/**
 * Fixtures for the rename dialogs (`$lib/file-explorer/rename/`):
 * `extension-change` and `rename-conflict`.
 *
 * Raw copy on purpose: this module is dev-only and sits outside the i18n-enforced
 * areas, so fixture strings never reach the message catalog.
 */

import type { ConflictFileInfo } from '$lib/file-explorer/rename/rename-operations'
import { daysAgo, hoursAgo } from './relative-time'

/** Props of `ExtensionChangeDialog.svelte`, minus its callbacks. */
export interface ExtensionChangeFixture {
  oldExtension: string
  newExtension: string
}

/** Props of `RenameConflictDialog.svelte`, minus its callback. */
export interface RenameConflictFixture {
  renamedFile: ConflictFileInfo
  existingFile: ConflictFileInfo
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
}

/**
 * Keyed by the `rename-conflict` entry's state ids in `gallery-registry.ts`.
 *
 * Both directions are here because the dialog is a COMPARISON: it tints whichever
 * side is newer and whichever is larger, so a single state leaves half the
 * treatment unreviewed.
 */
export const renameConflictFixtures: Record<string, RenameConflictFixture | undefined> = {
  'newer-and-larger': {
    renamedFile: {
      name: '2026-07-14_stockholm-archipelago-sunrise-session_DSC09241_final.arw',
      size: 118_293_504,
      modifiedAt: hoursAgo(3),
    },
    existingFile: {
      name: '2026-07-14_stockholm-archipelago-sunrise-session_DSC09241_final.arw',
      size: 41_238_912,
      modifiedAt: daysAgo(214),
    },
  },
  'older-and-smaller': {
    renamedFile: {
      name: 'notes.md',
      size: 2_048,
      modifiedAt: daysAgo(430),
    },
    existingFile: {
      name: 'notes.md',
      size: 184_320,
      modifiedAt: hoursAgo(1),
    },
  },
}
