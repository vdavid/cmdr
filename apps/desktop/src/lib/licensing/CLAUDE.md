# Licensing module (frontend)

License validation, commercial reminders, and expiration modals. Licenses use Ed25519 signatures for offline validation;
the backend owns the crypto and grace/revalidation timing (`src-tauri/src/licensing/CLAUDE.md`), the frontend trusts the
values via IPC.

## Files

- `licensing-store.svelte.ts`: state, validation trigger, `pendingVerification`, `resetForTesting()`.
- `LicenseKeyDialog.svelte`: license key entry + details view, "Use a different key" reset flow.
- `CommercialReminderModal.svelte`: 30-day reminder for personal users.
- `ExpirationModal.svelte`: shown when a commercial license expires.
- `AboutWindow.svelte`: displays current license status.

## License types

- **Personal**: free forever. "Personal use only" in title bar. Commercial reminder every 30 days.
- **Commercial subscription**: $59/year. Server validation every 7 days, 30-day offline grace on network failure.
- **Commercial perpetual**: $199 one-time. No periodic validation. 3 years of updates.
- **Expired**: reverts to Personal behavior (not locked out). Shows the modal once, then behaves as Personal.

## Must-knows

- **Activation uses the verify/commit split.** `handleActivate` calls `verifyLicense()` (nothing stored), then
  `validateLicenseWithServer(transactionId)` (transaction ID passed explicitly because the key isn't stored yet), then
  decides whether to `commitLicense()`. Four outcomes: active → commit + onSuccess; expired → commit + inline expiry
  error (key valid, just expired); invalid (server returns `personal`) → DON'T commit, nothing stored; network error
  (`newStatus` null) → commit + fallback `LicenseStatus` from `LicenseInfo` with `pendingVerification` set. Don't add a
  path that stores the key before server validation.
- **`commit_license` writes `cached_license_status` but NOT `last_validation_timestamp`.** So `needs_validation()` stays
  true and the frontend derives `pendingVerification` from `hasLicenseBeenValidated()` until a real server validation
  writes the timestamp. When set, the validity row shows "Not yet verified" (yellow) with a 7-day hint.
- **`resetForTesting()` must stay in sync with `licenseState`.** Adding a field to `licenseState` means clearing it in
  `resetForTesting()`. Tests use this instead of `vi.resetModules()` to avoid the module re-parse penalty.
- **Classify activation errors by typed code, never English substrings.** `getFriendlyError` uses
  `parseActivationError(e)` to extract a `LicenseActivationError` with a `code` field (`badSignature`, `networkError`,
  …) and switches on it. The error message (red, `--color-error`) and help text (secondary) are separate `<p>` elements.
- **License details view uses `LicenseInfo`, not `getCachedStatus()`**, for org name and license type; only
  `validityText` (expiry dates) comes from `getCachedStatus()` (server-sourced). `isServerInvalid` is a safety net for
  the rare case where a stored key is later rejected during a 7-day re-validation (`existingLicense !== null` AND cached
  type `=== 'personal'`).
- **Mailto links use `openExternalUrl`** (via `@tauri-apps/plugin-opener`), never raw `<a href="mailto:">` (Tauri blocks
  that navigation). The email is also shown as copyable text with a Copy button.
- **Ed25519 public key is embedded** in backend `verification.rs` and must match the API server's private key.
- **`CMDR_MOCK_LICENSE` bypasses validation in debug builds only** (silently ignored in release). Values: `personal`,
  `personal_reminder`, `commercial`, `perpetual`, `expired`, `expired_no_modal`. Example:
  `CMDR_MOCK_LICENSE=commercial pnpm dev`; `personal_reminder` pops the reminder on launch without waiting 30 days.

## Development

- Reset trial (debug): `security delete-generic-password -s "com.veszelovszki.cmdr" -a "trial-*"`.
- Generate a test license key: see [API server CLAUDE.md](../../../../api-server/CLAUDE.md#generate-a-test-license-key).

Full details (decision rationale, `licenseState`-not-`$state` choice, full activation-outcome and pending-verification
flows): [DETAILS.md](DETAILS.md).
