# Licensing module (frontend)

License validation, commercial reminders, and expiration modals. Licenses use Ed25519 signatures for offline validation;
the backend owns the crypto and the offline policy (`src-tauri/src/licensing/CLAUDE.md`), the frontend trusts the values
via IPC.

## Files

- `licensing-store.svelte.ts`: state, validation trigger, `pendingVerification`, `resetForTesting()`.
- `LicenseKeyDialog.svelte`: license key entry + details view, "Use a different key" reset flow.
- `CommercialReminderModal.svelte`: 30-day reminder for personal users.
- `ExpirationModal.svelte`: shown when a commercial license expires.
- `AboutWindow.svelte`: displays current license status.
- `AcknowledgementsDialog.svelte` + `acknowledgements-trigger.svelte.ts`: credits the open-source libraries Cmdr ships.
  ❌ Never hand-edit `third-party-packages.gen.json`: a vendored icon, font, or snippet is credited in
  `scripts/check/checks/third-party-vendored.json`. See `DETAILS.md`.

## License types

- **Personal**: free. "Personal use only" in title bar. Commercial reminder every 30 days.
- **Commercial perpetual**: what's sold today. Valid offline forever; the 7-day check only learns a revocation, so it's
  never "Not yet verified" (`isPendingVerification` excludes it).
- **Commercial subscription**: **retired, but live.** ❌ Don't delete the type or its paths (period end + 30 days
  offline) while the one license on it runs, to 2028-09-16.
- **Expired**: reverts to Personal behavior (not locked out). Shows the modal once, then behaves as Personal.

❌ **An unmapped Paddle price ID mints `commercial_subscription`**, so a misconfigured price sells an expiring license
against the page's "yours forever" promise. Prices, wiring, and what each type grants: `docs/business/pricing.md`.

## Must-knows

- **All user-facing copy here lives in `messages/en/licensing.json`, resolved via `t`/`tString`/`<Trans>`**
  (`$lib/intl`), never hardcoded; `cmdr/no-raw-user-facing-string` covers `lib/licensing/` and `LicenseSection.svelte`.
  Dates are locale-formatted at the call site and passed in as preformatted `{date}` STRING params. Catalog conventions
  (doubled apostrophes, `<Trans>` tag snippets): `$lib/intl/messages/CLAUDE.md`. Parity net:
  `licensing-i18n-parity.test.ts`, which pins the reminder's price literal so it can't drift.
- **Activation uses the verify/commit split.** `handleActivate` calls `verifyLicense()` (nothing stored), then
  `validateLicenseWithServer(transactionId)` (transaction ID passed explicitly because the key isn't stored yet), then
  decides whether to `commitLicense()`. Four outcomes: active → commit + onSuccess; expired → commit + inline expiry
  error (key valid, just expired); invalid (server returns `personal`) → DON'T commit, nothing stored; network error
  (`newStatus` null) → commit + fallback `LicenseStatus` from `LicenseInfo` with `pendingVerification` set. Don't add a
  path that stores the key before server validation.
- **`commit_license` writes `cached_license_status` but NOT `last_validation_timestamp`.** So `needs_validation()` stays
  true and the frontend derives `pendingVerification` from `hasLicenseBeenValidated()` until a real server validation
  writes the timestamp. When set (never for perpetual), the validity row shows "Not yet verified" with a 7-day hint.
- **`resetForTesting()` must stay in sync with `licenseState`**: a new field there needs clearing here. Tests use it
  rather than `vi.resetModules()`, which costs a module re-parse.
- **Classify activation errors by typed code, never English substrings.** `getFriendlyError` uses
  `parseActivationError(e)` to extract a `LicenseActivationError` with a `code` field (`badSignature`, `networkError`,
  …) and switches on it. The error message (red, `--color-error`) and help text (secondary) are separate `<p>` elements.
- **License details reads org name and type from `LicenseInfo`**, and only `validityText` (expiry) from the
  server-sourced `getCachedStatus()`. `isServerInvalid` catches a stored key later rejected on a 7-day re-validation
  (`existingLicense !== null` AND cached type `'personal'`).
- **Mailto links use `openExternalUrl`** (via `@tauri-apps/plugin-opener`), never raw `<a href="mailto:">` (Tauri blocks
  that navigation).
- **`CMDR_MOCK_LICENSE` bypasses validation in debug builds only** (silently ignored in release). Values: `personal`,
  `personal_reminder`, `commercial`, `perpetual`, `expired`, `expired_no_modal`. Example:
  `CMDR_MOCK_LICENSE=commercial pnpm dev`; `personal_reminder` pops the reminder on launch without waiting 30 days.

## Development

- Reset trial (debug): `security delete-generic-password -s "com.veszelovszki.cmdr" -a "trial-*"`.
- Generate a test license key: see [API server CLAUDE.md](../../../../api-server/CLAUDE.md#generate-a-test-license-key).

Decisions and the activation flows: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
