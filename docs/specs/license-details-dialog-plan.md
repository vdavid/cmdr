# Redesign license details dialog

## Context

The "License details" dialog has two UX issues:
1. **Empty license key field** — when the user activated with a full crypto key (not a short code), `shortCode` is null and the dialog shows an empty disabled input as its centerpiece. This looks broken.
2. **No way to replace a license key** — `resetLicense` exists in code but is debug-only and not exposed in the UI. Users switching orgs, upgrading tiers, or troubleshooting have no self-service path.

Additionally, "Your license is active." is a vague fallback when `expiresAt` is missing on a commercial license.

## Changes

### 1. Restructure the existing-license view in `LicenseKeyDialog.svelte`

Replace the current layout (description + disabled input + validity text) with a key-value info display.

Rows by `status.type`:

**`commercial`:**
- **Organization**: `status.organizationName` (omit row if null)
- **License type**: derived from `status.licenseType` — "Commercial subscription" or "Commercial perpetual"
- **Validity**: "Valid until {date}" for subscriptions, "Perpetual — updates until {date}" for perpetual with `expiresAt`, "Perpetual" if no `expiresAt`, "Active" only as absolute last resort
- **License key**: `existingLicense.shortCode` if available, otherwise omit the row entirely

**`supporter`:**
- **License type**: "Supporter"
- **License key**: `existingLicense.shortCode` if available, otherwise omit

**`expired`:**
- **Organization**: `status.organizationName` (omit row if null)
- **Validity**: "Expired on {date}"
- **License key**: `existingLicense.shortCode` if available, otherwise omit

Note: the `expired` variant doesn't carry `licenseType`, so we omit the type row for expired licenses.

Use a simple vertical list styled similarly to `SettingRow.svelte` — label on left in `--color-text-secondary`, value on right in `--color-text-primary`. Use `--color-border-subtle` dividers between rows. Wrap in a `--color-bg-tertiary` rounded box like `AboutWindow`'s `.license-info`.

### 2. Add "Use a different key" action

Add a secondary/text-style button below the info box. Flow:
- User clicks "Use a different key"
- Inline confirmation appears (replace the info box with a warning message: "This will deactivate your current license on this device. You can reactivate anytime with a valid key." + "Continue" / "Cancel")
- On confirm: call `resetLicense()`, then `loadLicenseStatus()` to update cached state, then reset the dialog to the "Enter license key" state (set `existingLicense = null`, focus the input)

This keeps everything in one dialog — no extra modal.

### 3. Make `resetLicense` work in release builds

Currently `reset_license` in `app_status.rs` is a no-op in release. Change the release build to also clear the license data. Keep the same logic: clear in-memory cache + delete all keys from `license.json`.

Also fix the existing gap: `reset_license` doesn't delete `STORE_KEY_SHORT_CODE` ("license_short_code"). Add that.

### 4. Refresh window title after reset-then-close

`onClose` in `+page.svelte` currently doesn't refresh the window title — only `onSuccess` does. If a user resets their license and closes without entering a new key, the title bar would be stale. Add `windowTitle = await getWindowTitle()` to `handleLicenseKeyDialogClose` so the title always reflects current state on dialog close.

### 5. Add a human-readable license type label helper

Add a small helper function in the dialog that maps `status` to a display string:
- `status.type === 'commercial'` + `status.licenseType === 'commercial_subscription'` -> "Commercial subscription"
- `status.type === 'commercial'` + `status.licenseType === 'commercial_perpetual'` -> "Commercial perpetual"
- `status.type === 'supporter'` -> "Supporter"
- `status.type === 'expired'` -> omit (no type row shown)

### 6. Fix activation bug after license reset

After reset + reactivation, the About window shows "No license" because:
- `activate_license` writes the key but not `cached_license_status`
- `validateLicenseWithServer()` fails in dev (no server) and falls back to `get_app_status()`
- `get_app_status()` → `get_cached_or_validate()` finds no cached status (reset deleted it) → returns `Personal`
- `handleActivate` discards the return value of `validateLicenseWithServer()` and calls `loadLicenseStatus()` separately

**Fix A (frontend):** In `handleActivate`, use the return value of `validateLicenseWithServer()` directly via a new
`setCachedStatus()` store function, instead of calling `loadLicenseStatus()`. This mirrors what `triggerValidationIfNeeded`
already does correctly in the store.

**Fix B (backend hardening):** In `activate_license_internal` (verification.rs), after storing the key, also write an
initial `cached_license_status` to the store. The payload already has `license_type` and `organization_name`. This
prevents the fallback-to-Personal path for any caller, not just the frontend.

### 7. Visual polish

**Button alignment:** The codebase has two conventions — `.actions` with `flex-end` (form-like dialogs) and `.button-row`
with `center` (binary-choice/confirmation dialogs). The confirmation state is a binary choice, so it should center-align.
The details view with "Use a different key" is form-like, so it keeps `flex-end`.

**"Use a different key":** Change from text link to `<Button variant="secondary">` for native app feel.

**Spacing:** Increase padding in the info box and row gaps so the layout breathes.

## Files to modify

| File | Change |
|---|---|
| `apps/desktop/src/lib/licensing/LicenseKeyDialog.svelte` | Restructure view, "Use a different key" flow, fix activation bug (use returned status), visual polish |
| `apps/desktop/src/lib/licensing/licensing-store.svelte.ts` | Add `setCachedStatus()` export |
| `apps/desktop/src-tauri/src/licensing/app_status.rs` | Remove debug gate on `reset_license`, add short code deletion |
| `apps/desktop/src-tauri/src/licensing/verification.rs` | Remove debug gate on `clear_license_cache`, write initial cached status on activation |
| `apps/desktop/src/routes/(main)/+page.svelte` | Add window title refresh to `handleLicenseKeyDialogClose` |

## Files unchanged

- `licensing.ts` — the `resetLicense()` TS wrapper already exists and works

## Verification

Use a real Paddle test license (any type) for manual testing. Mock mode only overrides `getCachedStatus()`, not
`getLicenseInfo()`, so the details view won't render without a real stored key.

1. Activate a test license, open License details — verify key-value layout shows org, license type, validity date
2. If activated via short code: verify the license key row shows the short code
3. If activated via full crypto key: verify the license key row is omitted (no empty field)
4. Test "Use a different key": click it, see confirmation, click "Continue", verify dialog switches to activation input
5. After reset + close (without entering a new key), verify window title reverts to personal-use state
6. Re-activate to confirm the full round trip works — About window must show the correct commercial status
7. Code review the supporter/expired template branches (not easily testable with real licenses, but low risk — just conditional text)
8. `./scripts/check.sh --check clippy --check svelte-check --check desktop-svelte-eslint --check desktop-svelte-prettier`
9. Update `apps/desktop/src/lib/licensing/CLAUDE.md` if any decisions/gotchas changed
