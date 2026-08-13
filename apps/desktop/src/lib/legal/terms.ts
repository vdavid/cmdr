/**
 * The terms the app asks the user to accept, and the one identifier that says WHICH terms
 * those were.
 *
 * The onboarding beta step records `TERMS_VERSION` alongside the acceptance timestamp
 * (`onboarding.termsAcceptedVersion` / `onboarding.termsAcceptedAt`), so a stored
 * acceptance always names the document it applied to. Consent to a superseded document
 * isn't consent to the current one, which is the whole reason the version is stored.
 *
 * **Bump `TERMS_VERSION` whenever the published terms change in a way that needs fresh
 * consent.** The value is the `lastUpdated` date of `apps/website/src/pages/terms-and-conditions.astro`
 * (ISO, YYYY-MM-DD), so the two stay checkable against each other by eye. Bumping it makes
 * every stored acceptance stale: the checkbox comes back unchecked on the next visit to the
 * beta step, and the user accepts again.
 */

/** ISO date (YYYY-MM-DD) of the currently published terms. See the module doc before changing. */
export const TERMS_VERSION = '2026-08-10'

/** The public terms page. Opened in the user's browser via `openExternalUrl`, never in-app. */
export const TERMS_URL = 'https://getcmdr.com/terms-and-conditions'
