/**
 * Fixtures for the updater's move-to-Applications nudge (`$lib/updates/`).
 *
 * The dialog's only content prop is which arrangement is keeping the update out of the bundle,
 * and each value renders a different first paragraph, so both get a state.
 */

import type { BundleWriteBlocker } from '$lib/tauri-commands'

/** Props of `MoveToApplicationsDialog.svelte`, minus its callback. */
export interface MoveToApplicationsFixture {
  blocker: BundleWriteBlocker
}

/** Keyed by the `move-to-applications` entry's state ids in `gallery-registry.ts`. */
export const moveToApplicationsFixtures: Record<string, MoveToApplicationsFixture> = {
  translocated: { blocker: 'translocated' },
  'read-only-volume': { blocker: 'readOnlyVolume' },
}
