/**
 * Fixtures for the device-troubleshooting dialogs (`$lib/mtp/`).
 *
 * Only `ptpcamerad` takes content props; `mtp-permission` takes callbacks only.
 *
 * Raw copy on purpose: this module is dev-only and sits outside the i18n-enforced
 * areas, so fixture strings never reach the message catalog.
 */

/** Props of `PtpcameradDialog.svelte`, minus its callbacks. */
export interface PtpcameradFixture {
  /** `undefined` renders the generic "something else is using it" copy. */
  blockingProcess?: string
}

/**
 * Keyed by the `ptpcamerad` entry's state ids in `gallery-registry.ts`. The two
 * states render different body copy (a named process vs the generic fallback),
 * which is the whole reason the prop is optional.
 */
export const ptpcameradFixtures: Record<string, PtpcameradFixture | undefined> = {
  'known-process': { blockingProcess: 'pid 45145, ptpcamerad' },
  unknown: {},
}
