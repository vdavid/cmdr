# Friendly error copy details

`CLAUDE.md` holds the must-knows. This file holds the depth: the data flow, the markdown-escaping contract, the test
strategy, and the convergence plan.

## Where this sits

This is one half of the friendly-error system. The other half is Rust classification
([`friendly_error/`](../../../src-tauri/src/file_system/volume/friendly_error/DETAILS.md)): it decides WHAT happened (a
typed `ListingError`), this decides the WORDS. The split realizes "smart backend, thin frontend" and seeds an i18n
catalog (a single home for all error copy).

The copy/move/delete WRITE-error prose is a sibling of this family but lives outside this directory: the
`errors.write.*` keys are in `errors.json` alongside ours, yet they're composed by
[`../file-operations/transfer/transfer-error-messages.ts`](../file-operations/transfer/transfer-error-messages.ts)
(rendered in `TransferErrorDialog`/`FallbackErrorContent`), not the factories here. Same `getMessage()`-not-ICU rule
(the values interpolate escaped paths/sizes and verb tokens, so they bypass ICU and use normal apostrophes). They're
parity-pinned by `transfer/transfer-error-messages.parity.test.ts`, not by our golden fixture. The two paths share the
`FriendlyErrorMessage` shape and may converge later.

## Data flow

1. Rust emits a `listing-error` event carrying a typed `ListingError` (category, reason + params, optional provider,
   action kind, retry hint, raw detail). Git failures ride as the `git` reason carrying a `FriendlyGitErrorKind`.
2. `ErrorPane` calls `renderListingError(error)` (`listing-error.ts`): it picks the base message from the listing
   factory (`getListingErrorMessage`) — or the git factory (`getGitErrorMessage`) for the `git` reason — then, when a
   `provider` is present, replaces the base suggestion with `getProviderSuggestion(provider, category)`. This reproduces
   the old Rust `enrich_with_provider` override exactly.
3. The factory pulls each string from the `errors.*` message catalog via `getMessage()` (a RAW catalog lookup, never ICU
   `t()` — see the catalog boundary below), substitutes its escaped runtime params (`{path}` / `{osMessage}` tokens),
   and substitutes localized macOS pane labels via `expandSystemStrings(...)`.
4. `ErrorPane` renders the result. The explanation / suggestion go through the single `renderErrorMarkdown` → snarkdown
   → `{@html}` site ([`error-pane-utils.ts`](../file-explorer/pane/error-pane-utils.ts)); the title and `raw_detail` are
   plain text.

## Markdown escaping (the XSS-load-bearing contract)

- **Why entities, not CommonMark `\` escapes.** snarkdown is a tiny non-CommonMark parser that does NOT honor backslash
  escapes (`STATUS\_DELETE\_PENDING` would render with visible backslashes). `escapeMarkdown` encodes markdown-special
  characters as HTML numeric entities, which snarkdown passes through untouched and the browser decodes on `{@html}`
  injection. `&` is encoded first so preexisting entity-like text is neutralized.
- **The escape set is conservative.** Line-start-only characters (`.`, `-`, `+`, `#`, `|`) are intentionally left alone:
  runtime values land mid-sentence in our templates, where those chars are inert, and escaping them would show ugly
  entities. This is a verbatim port of the former Rust `escape()` (same entity set, same carve-outs);
  `markdown-escape.test.ts` is the ported unit suite.
- **The invariant.** Every interpolated runtime param (path, OS message, device name, free-form provider text) passes
  through `esc(...)` before the template. Template literals are the only trusted markdown; the localized system-string
  labels are also trusted (they come from the OS loctable through our own backend) so `expandSystemStrings` does NOT
  escape them. Data-safety check: a path with markdown specials (e.g. `/Volumes/x/_todo_*pics`) must render literally,
  not as formatting.

## system_strings seam

Suggestions that name localized macOS panes (e.g. "Full Disk Access") use placeholder tokens (`{full_disk_access}`,
`{system_settings}`, `{privacy_and_security}`, `{files_and_folders}`, `{local_network}`, `{appearance}`) in the
templates; `expandSystemStrings` substitutes the live localized labels from `lib/system-strings.svelte.ts` (the
`get_localized_system_strings` backend snapshot). This mirrors the Rust `system_strings::expand`.

## Message-catalog boundary (`getMessage`, not ICU `t()`)

The literal English lives in [`../intl/messages/en/errors.json`](../intl/messages/en/errors.json) under `errors.*`, and
the factories resolve it via `getMessage(key)` — a raw catalog lookup with no ICU parsing. This is deliberate: error
values carry markdown plus `{system_settings}`-style `expandSystemStrings` tokens and `esc()` HTML entities, all of
which collide with ICU MessageFormat's brace/apostrophe grammar. Routing them through `t()`/`format()` would mangle
them. So errors are the one tranche that bypasses ICU (see the i18n plan's Decision 2 and the errors tranche note). A
corollary: `errors.json` values do NOT double apostrophes — the ICU `''` rule does not apply to non-ICU strings.

Key shape: `errors.listing.<reason>.{title,explanation,suggestion}`, `errors.git.<kind>.{title,message,suggestion}`, and
the provider keys. Param tokens (`{path}`, `{osMessage}`) are substituted by `interpolate(...)` in
`listing-error-messages.ts` AFTER `esc(...)`, on a token set disjoint from `expandSystemStrings`' so the two compose in
any order. The keys are built dynamically from the reason/kind/provider discriminant; TypeScript still proves each
resolves to a valid `MessageKey` (the catalog covers every discriminant). The cost is that the codegen's static usage
scan can't see them, so all `errors.*` keys show in the (non-fatal) dead-key report.

### Provider suggestion catalog layout

Most cloud providers share ONE template, `errors.provider.appBased.{transient,needsAction,serious}`, with `{name}` (the
bold display name) and `{app}` (the desktop-app name) tokens that `getProviderSuggestion` fills (provider names are
trusted, so unescaped). Bespoke providers (`macDroid`, `iCloud`, `macFuse`, `pCloudFuse`, `veraCrypt`, plus the two
collapsed ones) have their own per-category keys; `cmVolumes` and `genericCloudStorage` collapse needs_action + serious
into a single `nonTransient` key (they rendered identical copy for both). The `needs_action` category maps to the
`needsAction` catalog leaf (the key-naming check requires lowerCamel leaves). Display/app names live in
`errors.provider.<provider>.displayName` / `.appName`; the four providers without a distinct app (`macFuse`, `iCloud`,
`cmVolumes`, `genericCloudStorage`) have no `appName` key.

## Test strategy

- **`friendly-error-parity.test.ts`** (the behavior-preservation net): asserts these factories reproduce
  `__fixtures__/friendly_error_golden.json` byte-for-byte (one assertion per golden key). The golden was generated once
  by a temporary Rust test that recorded the exact pre-change rendered strings for a representative matrix (every errno
  arm, every typed `VolumeError` variant, every git kind, the empty-root case, every provider × category). The fixture
  is FROZEN: if the test fails, the FE copy drifted — fix the factory, never regenerate the fixture.
- **`friendly-error-style.test.ts`**: iterates every listing reason × representative params, every git kind, and every
  provider × category, asserting the rendered output obeys the writing rules (no "error" / "failed" / trivializing
  words). Strictly better coverage than the old Rust string test: it checks the actual rendered output. The one
  pre-existing nit ("for just this folder" in `tccRestricted`) is exempted via `PREEXISTING_TRIVIALIZING_EXCEPTIONS` —
  flagged for a future copy pass, not reworded (behavior-preserving move).
- **`markdown-escape.test.ts`**: the ported escaper unit tests (the security boundary).
- **`listing-error.test.ts`**: the adapter (base-message selection + provider override).

## Convergence (future)

The listing factory and `transfer-error-messages.ts` share several reasons (not-found, permission, already-exists,
device-disconnected, connection, io). Both already use the `FriendlyErrorMessage` shape so they CAN converge on one
catalog later. They are NOT force-merged yet: `getUserFriendlyMessage` (write) and `renderListingError` (listing) stay
separate entry points. The goal so far is one shape, not yet one function.
