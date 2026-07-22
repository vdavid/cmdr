/**
 * Fixtures for the licensing dialogs (`$lib/licensing/`).
 *
 * Only `expiration` takes content props. `about`, `license`, and
 * `commercial-reminder` take callbacks only and read everything else from the
 * licensing store and IPC, so they have no fixture data (their gallery rows say
 * so).
 *
 * Raw copy on purpose: this module is dev-only and sits outside the i18n-enforced
 * areas, so fixture strings never reach the message catalog.
 */

/** Props of `ExpirationModal.svelte`, minus its callback. */
export interface ExpirationFixture {
  organizationName: string | null
  /** ISO-8601 instant, exactly what `LicenseStatus.expiredAt` carries. */
  expiredAt: string
}

/**
 * Keyed by the `expiration` entry's state ids in `gallery-registry.ts`. Values
 * are optional so a lookup by an id that drifted out of the registry is
 * detectable rather than silently typed as present.
 */
export const expirationFixtures: Record<string, ExpirationFixture | undefined> = {
  // A long organization name is the layout risk here: it lands mid-sentence in
  // the body copy, so a tidy "Acme Inc" would hide the wrap.
  organization: {
    organizationName: 'Rymdskottkärra AB (Nordics, Baltics & Benelux division)',
    expiredAt: '2026-06-30T23:59:59Z',
  },
  personal: {
    organizationName: null,
    expiredAt: '2026-01-08T12:00:00Z',
  },
}
