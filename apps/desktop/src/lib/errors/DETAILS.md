# Friendly error copy details

`CLAUDE.md` holds the must-knows. This file holds the depth: the data flow, the markdown-escaping contract, the test
strategy, and the convergence plan.

## Where this sits

This is one half of the friendly-error system. The other half is Rust classification
([`friendly_error/`](../../../src-tauri/src/file_system/volume/friendly_error/DETAILS.md)): it decides WHAT happened (a
typed `ListingError`), this decides the WORDS. The split realizes "smart backend, thin frontend" and seeds an i18n
catalog (a single home for all error copy).

## Data flow

1. Rust emits a `listing-error` event carrying a typed `ListingError` (category, reason + params, optional provider,
   action kind, retry hint, raw detail). Git failures ride as the `git` reason carrying a `FriendlyGitErrorKind`.
2. `ErrorPane` calls `renderListingError(error)` (`listing-error.ts`): it picks the base message from the listing
   factory (`getListingErrorMessage`) — or the git factory (`getGitErrorMessage`) for the `git` reason — then, when a
   `provider` is present, replaces the base suggestion with `getProviderSuggestion(provider, category)`. This reproduces
   the old Rust `enrich_with_provider` override exactly.
3. The factory composes each string from a trusted template literal plus runtime params escaped with `esc(...)`, and
   substitutes localized macOS pane labels via `expandSystemStrings(...)`.
4. `ErrorPane` renders the result. The explanation / suggestion go through the single `renderErrorMarkdown` → snarkdown
   → `{@html}` site ([`error-pane-utils.ts`](../file-explorer/pane/error-pane-utils.ts)); the title and `raw_detail` are
   plain text.

The write path is a sibling, not this module: `transfer-error-messages.ts` renders `write-error` copy directly from the
typed `WriteOperationError`. The two share the `FriendlyErrorMessage` shape (see Convergence below).

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
