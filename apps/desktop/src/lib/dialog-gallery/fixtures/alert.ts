/**
 * Fixtures for the `alert` dialog (`$lib/ui/AlertDialog.svelte`).
 *
 * Raw copy on purpose: this module is dev-only and sits outside the i18n-enforced
 * areas, so fixture strings never reach the message catalog.
 */

export interface AlertFixture {
  title: string
  message: string
  buttonText?: string
}

/**
 * Keyed by the `alert` entry's state ids in `gallery-registry.ts`. Values are
 * optional because a lookup by an id that drifted out of the registry must be
 * detectable, not silently typed as present.
 */
export const alertFixtures: Record<string, AlertFixture | undefined> = {
  short: {
    title: 'Nothing to copy',
    message: 'Select at least one file first.',
  },
  long: {
    title: 'We couldn’t finish reading this folder',
    message:
      'The volume disconnected partway through, so the listing you see may be incomplete. Reconnect the drive and refresh the pane to get the full contents. If the drive keeps dropping, try a different cable or port before assuming the disk is at fault.',
  },
  'custom-button': {
    title: 'Indexing paused',
    message: 'Cmdr paused indexing because the drive is running on battery.',
    buttonText: 'Got it',
  },
  // Long unbroken tokens are where dialog layouts fall apart: no spaces to wrap on.
  'long-unbroken-path': {
    title: 'Path is too long',
    message:
      'This path doesn’t fit: /Volumes/Naspolya/media/photos/2026/07-summer-archive/raw-originals/Sony-A7RV/2026-07-14_stockholm-archipelago-sunrise-session/DSC09241_edited_final_v3_reallyfinal.arw',
  },
}
