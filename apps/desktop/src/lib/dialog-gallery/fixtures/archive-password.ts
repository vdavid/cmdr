/**
 * Fixtures for `archive-password` (`#lib/file-operations/transfer/UnlockDialog.svelte`),
 * the one prompt for every locked container: an encrypted archive or a restic
 * backup repo.
 *
 * Raw copy on purpose: this module is dev-only and sits outside the i18n-enforced
 * areas, so fixture strings never reach the message catalog.
 */

import type { UnlockContainer } from '#lib/file-explorer/pane/dialog-props.js'

/** Props of `UnlockDialog.svelte`, minus its callbacks. */
export interface ArchivePasswordFixture {
  container: UnlockContainer
  name: string
  wrongAttempt: boolean
  checking?: boolean
  unreachable?: boolean
}

/**
 * Keyed by the `archive-password` entry's state ids in `gallery-registry.ts`.
 * Each container has its own title and body copy, and the first prompt and the
 * re-prompt differ again, so every pair needs reviewing. A re-prompt opens on a
 * refusal: the title changes, a note sits under the red-ringed field, and the
 * field shakes once (not under Reduce motion). A repo adds the
 * Keychain checkbox, the busy state while it checks the password, and the note
 * for a check that couldn't reach it.
 */
export const archivePasswordFixtures: Record<string, ArchivePasswordFixture | undefined> = {
  'first-attempt': {
    container: 'archive',
    name: 'test-archive-2019-2025.zip',
    wrongAttempt: false,
  },
  // A long archive name rides into the body copy, which is where the 400px-wide
  // dialog either wraps gracefully or doesn't.
  'wrong-attempt': {
    container: 'archive',
    name: 'Familjefoton_backup_2011-2026_originalstorlek_lösenordsskyddad.zip',
    wrongAttempt: true,
  },
  'repo-first-attempt': {
    container: 'resticRepo',
    name: 'test-restic-repo',
    wrongAttempt: false,
  },
  'repo-checking': {
    container: 'resticRepo',
    name: 'test-restic-repo',
    wrongAttempt: false,
    checking: true,
  },
  'repo-wrong-attempt': {
    container: 'resticRepo',
    name: 'familjens-säkerhetskopior-restic-2019-2026',
    wrongAttempt: true,
  },
  'repo-unreachable': {
    container: 'resticRepo',
    name: 'test-restic-repo',
    wrongAttempt: false,
    unreachable: true,
  },
}
