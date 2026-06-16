# Friendly error copy (the words)

The factories that turn a typed error into the title / explanation / suggestion the user reads. Error CLASSIFICATION
lives in Rust ([`friendly_error/CLAUDE.md`](../../../src-tauri/src/file_system/volume/friendly_error/CLAUDE.md)): it
ships a typed, word-free `ListingError` (reason + params + category + detected provider + retry/action hints) over IPC.

The literal English lives in the `errors.*` catalog
([`../intl/messages/en/errors.json`](../intl/messages/en/errors.json)); each factory pulls its strings via
`getMessage('errors.<reason>.<part>')`. Editing copy means editing `errors.json` + `pnpm intl:keys`, not these `.ts`
files.

Writing rules for the copy: [`docs/style-guide.md`](../../../../../docs/style-guide.md) (active voice, friendly, never
the words "error" or "failed", no trivializing "just/simple/easy").

## Module map

- `listing-error-messages.ts`: `getListingErrorMessage(reason)` — resolves `errors.listing.<reason>.*` and interpolates
  the escaped `{path}` / `{osMessage}` param tokens.
- `git-error-messages.ts`: `getGitErrorMessage(kind)` — resolves `errors.git.<kind>.*` (static copy, no params).
- `provider-error-messages.ts`: `getProviderSuggestion(provider, category)` — resolves `errors.provider.*` (shared
  `appBased.*` template with `{name}`/`{app}` tokens, plus bespoke per-provider keys; see DETAILS.md).
- `listing-error.ts`: `renderListingError(error)` — the wire-`ListingError` → displayable adapter `ErrorPane` calls
  (picks the base message, applies the provider override).
- `markdown-escape.ts`: `escapeMarkdown` — the XSS boundary (verbatim port of the old Rust escaper).
- `compose.ts`: `esc(...)` (escape a param) + `expandSystemStrings(...)` (localized macOS pane labels).
- `friendly-error-message.ts`: the shared `FriendlyErrorMessage` shape (matches `transfer-error-messages.ts` so the two
  paths can converge later).

## Must-knows

- **Error strings resolve via `getMessage()`, NEVER `t()`/ICU.** Their `{system_settings}` tokens and `esc()` HTML
  entities collide with ICU's brace/apostrophe grammar, so catalog values are PLAIN strings that bypass ICU — and do NOT
  double apostrophes (the ICU `''` rule doesn't apply; write them normally). The compose pipeline (catalog lookup →
  `interpolate` escaped `{path}`/`{osMessage}` → `expandSystemStrings` → snarkdown/`{@html}`) is unchanged.
- **The codegen dead-key report lists EVERY `errors.*` key** (keys are built dynamically, so the usage scanner can't see
  them). It's a non-fatal warning; `pnpm check` stays green. To find a truly-dead error key, read the factory logic, not
  this report. See [`../intl/messages/DETAILS.md`](../intl/messages/DETAILS.md) § Dead-key honesty.
- **`escapeMarkdown` is the XSS boundary.** The composed explanation / suggestion is `{@html}`-injected via
  `renderErrorMarkdown` → snarkdown ([`error-pane-utils.ts`](../file-explorer/pane/error-pane-utils.ts)). EVERY
  interpolated runtime value (path, OS message, device name, free-form provider text) MUST pass through `esc(...)`
  before landing in a template. Template literals are the only trusted markdown; params are never trusted. The localized
  system-string labels (`expandSystemStrings`) are trusted (OS loctable via our backend), so they are NOT escaped.
- **The reason / provider / git names are the IPC contract with Rust.** Each `ListingErrorReason` member, `Provider`
  variant, and `FriendlyGitErrorKind` must match its Rust counterpart member-for-member. Drift breaks the parity test
  (and silently mis-renders at runtime).
- **The frozen golden fixture + parity test is the behavior-preservation net.** `friendly-error-parity.test.ts` asserts
  these factories reproduce `__fixtures__/friendly_error_golden.json` byte-for-byte (one case per golden key). If it
  fails, the FE copy drifted: fix the factory, do NOT regenerate the fixture. `friendly-error-style.test.ts` enforces
  the writing rules over every reason, every provider × category, and every git kind.
- **This is a behavior-preserving home, not a copy redesign.** Don't reword / merge / "improve" messages here as a side
  effect; a copy pass is separate. (One known pre-existing nit, "for just this folder" in `tccRestricted`, is preserved
  verbatim and exempted in the style test.)

## Adding a new error message

FE side (the Rust side is in
[`friendly_error/CLAUDE.md`](../../../src-tauri/src/file_system/volume/friendly_error/CLAUDE.md)): add the
`errors.<reason>.{title,explanation,suggestion}` keys (plus `@key` descriptions) to
[`errors.json`](../intl/messages/en/errors.json), run `pnpm intl:keys`, then add the new reason to the union in the
factory (the compose pipeline reads its keys automatically; a runtime param goes in the variant and is escaped + named
as a `{token}` in the catalog value). Add the reason to the parity + style test matrices. Full recipe + the convergence
note: [DETAILS.md](DETAILS.md).

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
