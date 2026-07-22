/**
 * Fixtures for `archive-password` (`$lib/file-operations/transfer/ArchivePasswordDialog.svelte`).
 *
 * Raw copy on purpose: this module is dev-only and sits outside the i18n-enforced
 * areas, so fixture strings never reach the message catalog.
 */

/** Props of `ArchivePasswordDialog.svelte`, minus its callbacks. */
export interface ArchivePasswordFixture {
  archiveName: string
  wrongAttempt: boolean
}

/**
 * Keyed by the `archive-password` entry's state ids in `gallery-registry.ts`.
 * The two states use different titles AND different body copy, so both need
 * reviewing.
 */
export const archivePasswordFixtures: Record<string, ArchivePasswordFixture | undefined> = {
  'first-attempt': {
    archiveName: 'tax-returns-2019-2025.zip',
    wrongAttempt: false,
  },
  // A long archive name rides into the body copy, which is where the 400px-wide
  // dialog either wraps gracefully or doesn't.
  'wrong-attempt': {
    archiveName: 'Familjefoton_backup_2011-2026_originalstorlek_lösenordsskyddad.zip',
    wrongAttempt: true,
  },
}
