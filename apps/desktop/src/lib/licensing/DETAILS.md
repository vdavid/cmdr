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

## Acknowledgements dialog

Credits every third-party package Cmdr ships. Opened from the Cmdr menu on macOS (`app.acknowledgements`), Help on
Linux, and from the command palette.

Its list is `third-party-packages.gen.json`, generated by the `third-party-notices` check from `Cargo.lock` and
`pnpm-lock.yaml` alongside the repo-root `THIRD-PARTY-NOTICES.md`. **Never hand-edit it**: the check regenerates it and
fails CI on drift. Full license texts live only in the notices file; the JSON carries names, versions, licenses, and
URLs.

Shape decisions worth keeping:

- **Loaded with a dynamic import on open**, not at startup. It's ~119 KB of JSON nothing else needs, so Vite code-splits
  it and the app doesn't carry it into first paint.
- **Every link is a `LinkButton` with an `href`**, and every click is intercepted (`preventDefault` +
  `openExternalUrl`). `LinkButton` owns the app's only sanctioned `cursor: pointer` and the one
  `svelte/no-navigation-without-resolve` opt-out, so feature code never hand-rolls either; the `href` stays for screen
  readers and right-click "Copy link" while the opener plugin does the actual opening. Rows override only
  `text-decoration` (underline on hover, not at rest): hundreds of permanently underlined names read as noise.
- **Fixed size, no `resizable`.** `containerStyle` pins `width: 644px` (min = max) and `height: 80vh`. `fillBody` makes
  the panel a flex column, and the body is a three-part column inside it: David's thank-you note and the two jump
  buttons on top, the THIRD-PARTY-NOTICES link pinned at the bottom, and only `.packages-scroll` between them scrolling.
  Scrolling the whole body instead would push the note and the notices link (the legally load-bearing pointer) out of
  sight the moment you move the list. `.packages-scroll` pulls itself out by `--spacing-sm` and pads the same amount
  back, so `.packages`' negative margin lands its margin box exactly on that padding box: the striped rows keep the
  dialog's left edge, the scrollbar rides just inside the panel edge, and nothing overflows sideways.
- **One grid spans both lists.** `.packages` is `grid-template-columns: minmax(0, 1fr) auto auto` (name takes the slack,
  version and license size to content); each `ul` and each `li` is `grid-template-columns: subgrid` spanning `1 / -1`.
  That nested subgrid is what makes the crate list and the npm list share identical column widths; a per-list grid would
  measure them separately and they'd drift.

The two lists render from one `{#each}` over a `sections` array rather than a snippet called twice: `{@render}` of a
void snippet trips `@typescript-eslint/no-confusing-void-expression`, and a separate child component would need covering
in `licensing.a11y.test.ts` too (`a11y-coverage` requires every `lib/` component to be exercised). The section headings
are bound into a `headings` record by section key so either jump button can `scrollIntoView` its heading
(`behavior: 'auto'` under `prefers-reduced-motion: reduce`); `jumpTo(key)` returns the handler, so the two buttons
differ only by key. They share one full-width 50/50 grid row above the scroll region, which reads as a pair of section
tabs, which is what they are.

**Testing gotcha**: the package list arrives from an `import()` that settles over an unknown number of macrotasks, so a
fixed count of `tick()`s silently leaves the dialog in its loading state. Both test files poll for `.package-list li`
instead.
