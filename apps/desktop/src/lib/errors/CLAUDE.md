# Friendly error copy (the words)

The canonical home of user-facing error copy. Error CLASSIFICATION lives in Rust
([`friendly_error/CLAUDE.md`](../../../src-tauri/src/file_system/volume/friendly_error/CLAUDE.md)): it ships a typed,
word-free `ListingError` (reason + params + category + detected provider + retry/action hints) over IPC. These factories
turn that into the title / explanation / suggestion the user reads.

Writing rules for the copy: [`docs/style-guide.md`](../../../../../docs/style-guide.md) (active voice, friendly, never
the words "error" or "failed", no trivializing "just/simple/easy").

## Module map

- `listing-error-messages.ts`: `getListingErrorMessage(reason)` — the per-reason factory (one case per
  `ListingErrorReason`).
- `git-error-messages.ts`: `getGitErrorMessage(kind)` — the parallel git factory (one case per `FriendlyGitErrorKind`).
- `provider-error-messages.ts`: `getProviderSuggestion(provider, category)` — the `(provider, category)` suggestion
  table plus provider display / app names.
- `listing-error.ts`: `renderListingError(error)` — the wire-`ListingError` → displayable adapter `ErrorPane` calls
  (picks the base message, applies the provider override).
- `markdown-escape.ts`: `escapeMarkdown` — the XSS boundary (verbatim port of the old Rust escaper).
- `compose.ts`: `esc(...)` (escape a param) + `expandSystemStrings(...)` (localized macOS pane labels).
- `friendly-error-message.ts`: the shared `FriendlyErrorMessage` shape (matches `transfer-error-messages.ts` so the two
  paths can converge later).

## Must-knows

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
[`friendly_error/CLAUDE.md`](../../../src-tauri/src/file_system/volume/friendly_error/CLAUDE.md)): add the factory case
for the new reason (in `listing-error-messages.ts`, or `git-error-messages.ts` for a git kind), escaping every runtime
param with `esc(...)`. Add the reason to the parity + style test matrices. Full recipe + the convergence note:
[DETAILS.md](DETAILS.md).

Full details: [DETAILS.md](DETAILS.md).
