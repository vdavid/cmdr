# Friendly error copy (the words)

The factories that turn a typed error into the title / explanation / suggestion the user reads. Error CLASSIFICATION
lives in Rust (`crates/cmdr-fs/src/volume/friendly_error/CLAUDE.md`): it ships a typed, word-free `ListingError`
(reason + params + category + detected provider + retry/action hints) over IPC.

The literal English lives in the `errors.*` catalog (`../intl/messages/en/errors.json`); each factory pulls its strings
via `getMessage('errors.<reason>.<part>')`. Editing copy means editing `errors.json` + `pnpm intl:keys`, not these `.ts`
files. Writing rules: `docs/style-guide.md` (active voice, friendly, never the words "error" / "failed", no
"just/simple/easy").

## Module map

- `listing-error-messages.ts`: `getListingErrorMessage(reason)` resolves `errors.listing.<reason>.*` and interpolates
  the escaped `{path}` / `{osMessage}` param tokens.
- `git-error-messages.ts`: `getGitErrorMessage(kind)` resolves `errors.git.<kind>.*` (static copy, no params).
- `provider-error-messages.ts`: `getProviderSuggestion(provider, category)` resolves `errors.provider.*` (shared
  `appBased.*` template with `{name}`/`{app}` tokens, plus bespoke per-provider keys; see DETAILS.md).
- `listing-error.ts`: `renderListingError(error)` is the wire-`ListingError` → displayable adapter `ErrorPane` calls
  (picks the base message, applies the provider override).
- `markdown-escape.ts`: `escapeMarkdown`. `compose.ts`: `esc(...)` (escape a param) + `expandSystemStrings(...)`
  (localized macOS pane labels).
- `friendly-error-message.ts`: the shared `FriendlyErrorMessage` shape.
- `server-request.ts`: words a failed request to Cmdr's api server and picks its log level (`DETAILS.md`).

## Must-knows

- **Error strings resolve via `getMessage()`, NEVER `t()`/ICU.** Their `{system_settings}` tokens and `esc()` HTML
  entities collide with ICU's brace/apostrophe grammar, so catalog values are PLAIN strings that bypass ICU, and do NOT
  double apostrophes (the ICU `''` rule doesn't apply; write them normally). The compose pipeline (catalog lookup →
  `interpolate` escaped `{path}`/`{osMessage}` → `expandSystemStrings` → snarkdown/`{@html}`) is unchanged.
- **The orphan check can't see `errors.*` keys** (they're built dynamically), so `desktop-message-keys-unused` carries
  their prefixes on its dynamic allowlist. To find a truly-dead error key, read the factory logic. See
  `../intl/messages/DETAILS.md` § Dead-key honesty.
- **`escapeMarkdown` is the XSS boundary.** The composed explanation / suggestion is `{@html}`-injected via
  `renderErrorMarkdown` → snarkdown (`../file-explorer/pane/error-pane-utils.ts`). EVERY interpolated runtime value
  (path, OS message, device name, free-form provider text) MUST pass through `esc(...)` before landing in a template.
  Template literals are the only trusted markdown; params are never trusted. The localized system-string labels
  (`expandSystemStrings`) are trusted (OS loctable via our backend), so they are NOT escaped.
- **The reason / provider / git names are the IPC contract with Rust.** Each `ListingErrorReason` member, `Provider`
  variant, and `FriendlyGitErrorKind` must match its Rust counterpart member-for-member. Drift breaks the parity test
  (and silently mis-renders at runtime).
- **Three frozen goldens are the behavior net.** `friendly-error-parity.test.ts` replays `friendly-error-matrix.ts`
  against `__fixtures__/friendly_error_golden.json` (macOS) and its `.windows.json` / `.linux.json` siblings. A failure
  means drift: fix the factory or catalog, never regenerate (a deliberate reword hand-edits its values: `DETAILS.md`).
  `friendly-error-style.test.ts` holds every reason, provider, and git kind to the writing rules on all three OSes.
- **Per-OS words are a catalog `{platform, select, …}` block**, never a factory branch; `{system_settings}`-style tokens
  go in `macos` only. `DETAILS.md` § Platform branches.
- **Don't reword messages as a side effect**; a copy pass is separate (the "for just this folder" nit in `tccRestricted`
  is exempted in the style test).

## Adding a new error message

FE side: add the `errors.<reason>.{title,explanation,suggestion}` keys (plus `@key` descriptions) to
`../intl/messages/en/errors.json`, run `pnpm intl:keys`, translate into every locale (`docs/guides/i18n-translation.md`
§ New feature), add the reason to the factory union (a runtime param goes in the variant and is escaped + named as a
`{token}`), and add it to the STYLE matrix only. A new reason gets NO golden entry: the fixture pins pre-change output,
so there's nothing to pin it against. Rust side: `crates/cmdr-fs/src/volume/friendly_error/CLAUDE.md`.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
