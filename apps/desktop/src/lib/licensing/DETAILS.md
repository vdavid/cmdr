# Licensing details (frontend)

Depth and rationale. `CLAUDE.md` holds the must-knows; decision detail and the full flow live here. Backend depth is in
`src-tauri/src/licensing/DETAILS.md`.

## Key decisions

**Decision**: Ed25519 offline verification for all license types, server validation only for subscriptions. **Why**: A
file manager must work without internet. Perpetual and personal licenses validate purely offline via the Ed25519
signature. Subscriptions need periodic server checks (every 7 days) to detect cancellation, but get a 30-day offline
grace so intermittent network issues don't disrupt paid users.

**Decision**: Activation uses a verify/commit split (`verifyLicense` read-only, then `commitLicense` persist), with
server validation in between. **Why**: The old flow stored the key before server validation. If the server rejected it
and the user force-quit, the invalid key persisted. Now `verifyLicense` checks the signature without writing,
`validateLicenseWithServer` checks with the server (passing `transactionId` explicitly since the key isn't stored yet),
and `commitLicense` only runs when we want to keep the key. Invalid keys never touch disk.

**Decision**: Expired commercial licenses revert to Personal behavior (not locked out). **Why**: The app is usable for
free. Locking out a paying user whose subscription lapsed would be hostile. Instead the app quietly downgrades and shows
a one-time renewal modal; the user keeps working.

**Decision**: `licenseState` is a plain object (not `$state`) despite living in a `.svelte.ts` file. **Why**: The store
is consumed by layout-level code that reads `cachedStatus` and `shouldShowModal` imperatively, not reactively. Runes
would add reactivity overhead for state that only changes on explicit user actions (activate, dismiss). The About window
and modals read the cached value on mount.

## i18n migration

All licensing copy moved into `messages/en/licensing.json` (keys `licensing.about.*`, `licensing.commercialReminder.*`,
`licensing.expiration.*`, `licensing.dialog.*`, `licensing.error.*`, `licensing.section.*`), resolved through
`$lib/intl` (`tString` for plain/interpolated strings, `<Trans>` for sentences with inline components). It's a
behavior-preserving move: en output is byte-identical, pinned by `licensing-i18n-parity.test.ts`.

- **The About window keeps David's first-person voice** (the beta note "Tell me on GitHub. I read every report!").
  Translators are told to preserve that warmth via the `@key` description, not a positional flag.
- **Prices and proper names stay literal in the base string.** `$59/year/user`, `Falcon-H1R-7B`, `TII`, brand names, and
  the `CMDR-XXXX-XXXX-XXXX` format example are flagged do-not-translate in their `@key` descriptions; there is no price
  param (the amount is copy, not data).
- **Dates are formatted at the call site, then passed in as preformatted `{date}` STRING params** (the same
  single-source rule as `$lib/intl`), never via ICU `{date, date}`. Each component keeps its local `formatDate` helper.
- **Inline-component sentences use `<Trans>` with a tag snippet whose name differs from any param** to avoid the
  handler-overwrites-param collision: the contact-email lines use a `<supportEmail>` tag wrapping the `{email}` param
  (tag `supportEmail`, snippet bound `supportEmail={email}`), the expiration modal uses `<strong>`, the dismiss button a
  `<break>` line break, and the About/enter-key prompts a `<github>` / `<getLicense>` link tag.

## Activation outcomes (`handleActivate`)

`handleActivate` calls `verifyLicense()` first (nothing stored), then `validateLicenseWithServer(transactionId)` passing
the transaction ID explicitly, then decides whether to call `commitLicense()`:

1. Server confirms active (commercial) → `commitLicense()` + `onSuccess()`.
2. Server says expired → `commitLicense()` + inline error with expiry date (key IS valid, just expired).
3. Server says invalid (returns `personal` type) → DON'T commit. Nothing stored. Tracks `serverInvalidRetryCount` for
   escalating messaging. Cancel and X just close (no cleanup needed).
4. Network error (`newStatus` is null) → `commitLicense()` + a fallback `LicenseStatus` from `LicenseInfo`, calls
   `onSuccess()` with `pendingVerification` set.

## `pendingVerification` flag

Tracked in `licensing-store.svelte.ts`. Derived from backend state on startup: `hasLicenseBeenValidated()` returns false
when `last_validation_timestamp` is absent (license committed locally but never server-verified). Also set directly
during activation on the network-fallback path. Cleared when `triggerValidationIfNeeded` completes successfully.
Survives restarts because the backend state persists. When set, the validity row shows "Not yet verified" (yellow) with
a 7-day hint.

## Other gotchas

- **Commercial reminder timing**: tracked in `license.json` via `firstRunTimestamp`. Shows 30 days after first launch,
  then every 30 days.
- **Trial persistence via Keychain**: uses IOPlatformUUID (hashed). Survives reinstalls. Fresh trial on a new Mac.
- **Self-service deactivation**: "Use a different key" in `LicenseKeyDialog` calls `resetLicense()`, clearing all
  license data (key, short code, cached status, validation timestamp) and reverting to Personal.
- **`errorHelpHint` state**: holds context-specific help text from `getFriendlyError()` for non-server-invalid errors
  (signature failures, network issues), rendered in the separate secondary `<p>`.
