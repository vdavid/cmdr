# Move friendly-error text to the frontend

Move all user-facing PROSE out of the Rust `friendly_error` system and onto the Svelte frontend, while keeping error
CLASSIFICATION in Rust. This is step 1 of the larger "i18n-ready" effort, but it stands on its own: it realizes the
"all displayable text lives on the frontend, smart backend stays smart" principle. i18n-readiness is the free side
effect (a single frontend home for error copy that a catalog tool can later own).

## Goal

After this change:

- The Rust backend emits, for every error it reports to the UI, a TYPED classification (a `category`, a semantic
  `reason`, structured params, an optional detected `provider`, an optional `actionKind`, a `retryHint`, and a technical
  `rawDetail`) and ZERO user-facing prose.
- The frontend owns 100% of the user-facing words (titles, explanations, suggestions), rendered from that typed data,
  mirroring the pattern that `src/lib/file-operations/transfer/transfer-error-messages.ts` already uses for write
  errors.
- The `Markdown` newtype, the `md!` macro, the Rust markdown escaper, and the `bindings.ts` `Markdown` brand
  post-processing are all gone. Markdown escaping happens on the frontend, next to the single `snarkdown` render site.

## Non-goals (hold the line on scope)

- **This is a behavior-preserving move, not a copy redesign.** Every current distinct message must render byte-identical
  after the change (modulo the localized macOS pane-name interpolation, which already happens via `system_strings`). Do
  NOT reword, merge, or "improve" any message. Copy edits are a separate later pass.
- **Do NOT collapse the taxonomy for brevity.** One `reason` per currently-distinct message (errnos that already share
  identical copy collapse to one reason; nothing else merges). Preserving the 1:1 mapping is what makes the diff
  reviewable and the parity check meaningful.
- **Do NOT introduce Paraglide or any i18n library here.** Produce plain typed TS message factories in the exact shape
  of the existing `transfer-error-messages.ts`. Adopting a catalog tool is step 2 and becomes a mechanical lift once all
  copy lives in these factories.
- **Do NOT change error classification logic.** errno→reason mapping, the TCC-vs-permission branch, provider detection,
  category/retry assignment, the Layer-0 git pass-through ordering: all stay in Rust, unchanged in behavior.

## The one principle this encodes

Separate a message's IDENTITY (a typed key + structured params: the backend's job, needs the path, the errno, the
`statfs` type) from its WORDS (the frontend's job). The backend already does this correctly for write errors; this
change makes the listing path and the git path match.

## Background: current state (verified inventory)

Three error paths reach the UI today. Two bake prose in Rust; one already does it the target way.

- **Listing path** (the main work). `file_system/listing/streaming.rs` emits `listing-error` carrying
  `friendly: Option<FriendlyError>`. `FriendlyError` (in `friendly_error/mod.rs`) holds baked `title: String`,
  `explanation: Markdown`, `suggestion: Markdown`. Built by `friendly_error_from_volume_error` (`volume_error.rs`)
  dispatching to `errno.rs` (≈34 errno arms), `kinds.rs` (12 shared failure-class constructors), and an inline
  `IsADirectory` arm, then enriched by `enrich_with_provider` (`provider.rs`, 18 providers).
- **Empty-root path.** `empty_root.rs::friendly_error_for_restricted_empty_root` returns a baked `FriendlyError` for an
  iCloud-root-looks-empty-without-FDA hint. Emitted on the same `listing-error` event.
- **Git path (Layer 0).** `VolumeError::FriendlyGit(FriendlyGitError)` carries a `FriendlyGitErrorKind` (10 kinds, in
  `file_system/git/friendly.rs`) whose `to_friendly_error()` bakes prose. Matched first in
  `friendly_error_from_volume_error`, before errno mapping, and not provider-enriched.
- **Write path (already correct, mostly redundant).** `write-error` events carry both the typed
  `error: WriteOperationError` AND a redundant `friendly: Option<FriendlyError>` (built by `write_error.rs`). The
  frontend ALREADY renders text from the typed `error` via `transfer-error-messages.ts::getUserFriendlyMessage`. The
  `friendly` payload is only used as a fallback shape. Two FE gaps exist: `ReadOnlyDevice` and `DeletePending` fall
  through to the default case.

Markdown handling today: `explanation`/`suggestion` are `Markdown(String)`, built with `md!` which escapes interpolated
runtime values (paths, OS messages) into HTML entities so `snarkdown` doesn't mis-render them (e.g.
`STATUS_DELETE_PENDING` as italics). The wire type is branded in `bindings.ts` as
`string & { readonly __markdown: unique symbol }` via a string-replace in `ipc.rs:787-803`. The single FE render site is
`renderErrorMarkdown` in `file-explorer/pane/error-pane-utils.ts` (calls `snarkdown`); it is `{@html}`-injected, so
escaping is XSS-load-bearing.

## Design decisions

### Decision 1: the `reason` taxonomy (1:1 with current messages)

Introduce a typed, camelCase-serialized enum (working name `ListingErrorReason`) with exactly one variant per
currently-distinct listing/empty-root message. Rust's existing classification now outputs a `reason` instead of baked
prose; the FE switches on `reason` to pick the message factory. The frontend NEVER sees raw errno numbers (preserving
the existing "no errno knowledge in the FE" decision): Rust maps errno → semantic reason, the FE switches on the
semantic reason.

Derive the variant set mechanically from the current arms:

- The 12 `kinds.rs` constructors are the backbone (`notFound`, `tccRestricted`, `permissionDenied`, `alreadyExists`,
  `cancelled`, `deviceDisconnected`, `readOnly`, `storageFull`, `connectionTimedOut`, `notSupported`, `deletePending`,
  `ioSerious`). Plus the inline `isADirectory`. Plus the empty-root iCloud hint reason.
- Each errno arm that does NOT route through a `kinds` constructor and has its own distinct copy becomes its own reason
  (e.g. `EINTR` interrupted, `ENOMEM` not-enough-memory, `EBUSY` resource-busy, `EAGAIN` temporarily-unavailable, the
  network family `ENETDOWN`/`ENETRESET`/`ECONNABORTED`/`ECONNRESET`/`ETIMEDOUT`/`EHOSTDOWN`/`ESTALE`/`ENETUNREACH`/
  `ECONNREFUSED`/`EHOSTUNREACH`, `EXDEV` cross-device, `ENOTDIR`, `EISDIR`, `EROFS`, `ENOTSUP`, `ENAMETOOLONG`,
  `ENOTEMPTY`, `EDQUOT`, `EAUTH`/`ENEEDAUTH` auth-required, `EPWROFF`, `ENOATTR`, `EIO`, `EINVAL`, `EDEVERR`, plus the
  unknown-errno `couldntRead` fallback). Errnos that today produce IDENTICAL copy collapse to ONE reason; the
  implementer confirms exact current copy per arm and collapses only on true equality.

Each reason carries its params as typed fields on the wire (see Decision 4). The full reason→params and reason→category
/retry/actionKind table is mechanical: lift it verbatim from the current arms (the inventory in the spawning context
enumerates it; re-derive from source to be safe).

Git keeps its own typed enum: ship `FriendlyGitErrorKind` (already typed, 10 variants) across IPC as its own reason
namespace rather than folding it into `ListingErrorReason`. The FE renders git messages from a parallel git factory.
Rationale: git copy is git-domain, already cleanly typed, and the Layer-0 pass-through stays a distinct path.

### Decision 2: provider overlay (typed provider, FE composes)

`provider.rs` currently OVERWRITES the suggestion with provider-specific advice keyed by (provider, category). Keep
detection in Rust (it needs path patterns + `statfs`), move the words to the FE:

- Ship `provider: Option<Provider>` (typed camelCase enum, 18 variants) on the listing-error payload.
- The FE holds a `(provider, category) → suggestion` table (≈32 distinct strings). When `provider` is present, the FE
  uses the provider suggestion in place of the base reason's suggestion, reproducing today's override exactly.
- Provider display names / app names move to the FE table too (they are words). The Rust `Provider` enum keeps only the
  variant identity and detection logic; drop `display_name()`/`app_name()` from Rust (or keep only if still needed by
  non-UI code: verify no non-UI caller).

### Decision 3: markdown + escaping contract (the XSS-load-bearing part)

- Params cross IPC as PLAIN strings (no `Markdown` newtype). The FE composes each message by inserting params into a
  trusted template literal, escaping every param first, then passing the result through the existing single
  `renderErrorMarkdown` → `snarkdown` → `{@html}` site.
- Port the Rust `escape()` function (`friendly_error/markdown.rs`) to a small TS helper verbatim (same HTML-entity set,
  same line-start-char carve-outs) with its unit tests ported. This is the security boundary: state as an invariant
  that EVERY interpolated runtime value (path, OS message, device name, any free-form provider text) MUST pass through
  this escaper before reaching `snarkdown`. Template literals are the only trusted markdown; params are never trusted.
- Remove from Rust: the `Markdown` newtype, `MarkdownArg`, the `md!` macro, the `escape`/`is_md_special` functions, and
  the `ipc.rs:787-803` brand post-processing. Remove the `Markdown` type from `bindings.ts` (regenerate). `rawDetail`
  stays a plain `String` (it already is; it is rendered as plain text, not markdown: confirm and preserve that).

### Decision 4: the IPC wire shapes

- **Listing path.** Replace `friendly: Option<FriendlyError>` on `ListingErrorEvent` with a typed payload (working name
  `ListingError`) carrying: `category: ErrorCategory`, `reason: ListingErrorReason`, the reason's structured params
  (model these as data on the reason enum variants, e.g. serde-tagged variant fields, OR a flat struct with optional
  params: pick the shape that serializes cleanly through tauri-specta and is ergonomic to switch on in TS; prefer
  variant-carried data so impossible param combinations are unrepresentable), `provider: Option<Provider>`,
  `actionKind: Option<ErrorActionKind>`, `retryHint: bool`, `rawDetail: String`. Keep the existing top-level
  `message: String` on `ListingErrorEvent` if any consumer still needs it; otherwise remove it (verify consumers).
- **Empty-root path.** Same `ListingError` shape, with its own reason variant; emitted as today.
- **Git path.** Carry the typed `FriendlyGitErrorKind` + its `path`/`raw` across IPC (either as a `ListingError` with a
  git reason wrapping the kind, or a sibling typed field: choose the cleaner specta shape). No baked prose.
- **Write path.** Remove `friendly` from `WriteErrorEvent` entirely. The FE already renders from `error`. Before
  removing, close the two FE gaps in `transfer-error-messages.ts`: add explicit cases for `ReadOnlyDevice` (currently
  partial) and `DeletePending` (currently missing), matching the prose `write_error.rs` produces today. Then delete
  `write_error.rs` and `friendly_from_write_error`.

### Decision 5: classification stays in Rust, words on the FE (explicit split)

- **Rust keeps:** errno→reason mapping, the `kinds.rs` constructors (now returning a typed reason + params instead of a
  `FriendlyError`), the TCC-vs-permission branch (`tcc_paths::is_potentially_tcc_restricted` + network-path check),
  category/retry/actionKind assignment, `enrich_with_provider` detection (now setting `provider` instead of overwriting
  prose), the Layer-0 git pass-through and its ordering, and `system_strings` (the macOS pane-name source).
- **FE gains:** all titles/explanations/suggestions, the provider-suggestion table, provider display/app names, the
  reason/git/provider message factories, and markdown escaping + composition.
- **`system_strings` seam:** suggestions that interpolate localized macOS pane names (e.g. "Full Disk Access") are built
  on the FE using the already-existing `get_localized_system_strings` command and `lib/system-strings.svelte.ts`. The FE
  injects the localized label into its suggestion template. Verify the FE snapshot already exposes every pane name the
  current suggestions use (`system_settings`, `privacy_and_security`, `files_and_folders`, `full_disk_access`,
  `local_network`, `appearance`, plus any others the current arms reference such as Activity Monitor / Network / General
  / Storage: if a needed label is missing from the snapshot, ADD it to `system_strings.rs` and the FE snapshot rather
  than hardcoding English).

## Convergence note (bound the scope, but aim it right)

The listing factory and the existing `transfer-error-messages.ts` will share several reasons (not-found, permission,
already-exists, device-disconnected, connection, io). Structure the new code so they CAN converge later: reuse the
`FriendlyErrorMessage` interface (`{ title, message, suggestion }`), and put genuinely shared reason factories in a
common module both paths import. Do NOT force-merge the two entry points (`getUserFriendlyMessage` vs the new listing
renderer) in this pass: that is a follow-up. The goal here is one shape, not yet one function.

## Test strategy

- **Capture a golden snapshot FIRST (parity safety net).** Before touching anything, add a temporary Rust test that, for
  a representative input matrix (every `VolumeError` variant, every errno arm, every git kind, the empty-root case, each
  provider×category), records the current rendered `title`/`explanation`/`suggestion`. Save the output. After the FE
  migration, assert the FE factories reproduce the same strings for the same inputs (a mirrored TS snapshot test).
  Remove the temporary Rust snapshot test at the end. This is how the reviewer trusts "behavior-preserving".
- **Delete the Rust prose tests** that assert on `.title`/`.explanation`/`.suggestion` text: `friendly_error/mod.rs`
  prose tests, `provider.rs` suggestion-text tests, `git/friendly.rs` prose tests (≈850 lines of prose-assertion test
  code across these).
- **Add Rust mapping tests:** assert errno/variant/git-kind → correct `reason`, `category`, `retryHint`, `actionKind`,
  `provider`, and that the right params are populated. (The category/retry tests in `mod.rs` mostly survive: they assert
  category/retry, not prose: keep those, retarget to the new shape.)
- **Move the style-rule tests to the FE.** Port `error_messages_never_contain_error_or_failed` (and the trivializing-
  word checks) to a TS test that iterates EVERY reason × representative params (and every provider×category, every git
  kind) and asserts the rendered text obeys the writing rules (no "error"/"failed", no "just/simple/easy", etc.). This
  is strictly better coverage: it checks the actual rendered output.
- **Port the escaper unit tests** from `markdown.rs` to the TS escaper.
- **Extend `transfer-error-messages.test.ts`** for the two new cases (`ReadOnlyDevice`, `DeletePending`).
- **FE render tests:** `FriendlyErrorContent`, `ErrorPane`, `TransferErrorDialog` already have tests
  (`*.friendly.test.ts`, `FriendlyErrorContent.test.ts`, `error-pane-utils.test.ts`): update to the new typed input.

## Implementation sequence

Each step compiles and passes checks before the next.

1. **Golden snapshot.** Add the temporary Rust parity-snapshot test; save its output as the reference.
2. **Write path (smallest, proves the pattern).** Close the two FE gaps in `transfer-error-messages.ts`
   (`ReadOnlyDevice`, `DeletePending`) with prose matching today's `write_error.rs`. Remove `friendly` from
   `WriteErrorEvent`; delete `write_error.rs` + `friendly_from_write_error`. Regenerate bindings. Verify the transfer
   error UI is unchanged.
3. **Listing reason enum + Rust classification rewrite.** Introduce `ListingErrorReason` + the new `ListingError`
   payload. Rewrite `kinds.rs`, `errno.rs`, `volume_error.rs`, `empty_root.rs` to return typed reason+params instead of
   `FriendlyError`. Set `provider` from `enrich_with_provider` instead of overwriting prose.
4. **Git path.** Ship `FriendlyGitErrorKind` + params across IPC; stop baking prose in `git/friendly.rs`
   (`to_friendly_error` either goes away or returns the typed shape).
5. **Frontend factories.** Build the listing message factory (`reason → FriendlyErrorMessage`), the provider-suggestion
   table, the git factory, and the TS markdown escaper. Wire `ErrorPane` / `FriendlyErrorContent` to render from the new
   typed payload, composing + escaping markdown on the FE. Use `system-strings.svelte.ts` for pane names.
6. **Remove the Markdown machinery.** Delete the `Markdown` newtype, `MarkdownArg`, `md!`, `escape`/`is_md_special` from
   Rust; remove the `ipc.rs:787-803` brand block; regenerate bindings (the `Markdown` type disappears).
7. **Tests.** Delete Rust prose tests, add Rust mapping tests, port style + escaper tests to the FE, add the TS parity
   snapshot, extend transfer tests. Remove the temporary Rust golden-snapshot test once the TS parity test is green.
8. **Docs.** Update `friendly_error/CLAUDE.md` and `DETAILS.md`: the "mapping in Rust, not the frontend" decision becomes
   "CLASSIFICATION in Rust, WORDS on the frontend" with the new split and the FE escaping invariant. Update the markdown
   section (escaping now lives on the FE). Note the new home of the writing-rules tests. Touch
   `docs/architecture.md` only if a one-line pointer needs it (it is a map: no mechanism).

## Files in scope (from inventory; verify before editing)

Rust:
- `src-tauri/src/file_system/volume/friendly_error/{mod,errno,kinds,volume_error,write_error,empty_root,provider,markdown}.rs`
- `src-tauri/src/file_system/git/friendly.rs`
- `src-tauri/src/file_system/listing/streaming.rs` (`ListingErrorEvent`)
- `src-tauri/src/file_system/write_operations/types.rs` (`WriteErrorEvent`)
- `src-tauri/src/file_system/write_operations/event_sinks.rs` (error event construction)
- `src-tauri/src/ipc.rs` (remove Markdown brand block)

Frontend:
- `src/lib/ipc/bindings.ts` (regenerated, do not hand-edit)
- `src/lib/file-operations/transfer/transfer-error-messages.ts` (+ `.test.ts`)
- `src/lib/file-operations/transfer/{FriendlyErrorContent,FallbackErrorContent,TransferErrorDialog}.svelte` (+ tests)
- `src/lib/file-explorer/pane/{ErrorPane.svelte,error-pane-utils.ts}` (+ test)
- new: listing/git/provider message factories + the TS markdown escaper (place under `file-operations/transfer/` or a
  shared `lib/errors/` module: choose per the convergence note; a shared `lib/errors/` is the more elegant home and
  signals the future single catalog)
- `src/lib/system-strings.svelte.ts` (consume for pane names; extend if a label is missing)

## Verification (definition of done)

- `pnpm check` is green (Rust + Svelte + Go), including `pnpm bindings:regen` producing a `Markdown`-free `bindings.ts`.
- The TS parity snapshot reproduces every pre-change rendered message string (the temporary Rust golden snapshot and the
  TS snapshot agree), proving behavior is preserved.
- The style-rule test (no "error"/"failed"/trivializing words) passes over EVERY reason, provider×category, and git
  kind, on the FE.
- No `Markdown`, `md!`, or markdown-escaping code remains in Rust; no `friendly: Option<FriendlyError>` on either event.
- A manual spot-check of three representative errors (a permission-denied on a TCC path, a Dropbox timeout, a git
  "no repo here") renders identically to before, including the localized macOS pane name and the action button.

## Data-safety call-out for the reviewer

The escaper is the one piece that must not regress: it is the boundary between untrusted runtime strings (paths, OS
messages) and `{@html}`-injected markdown. Re-run the ported escaper tests and the FE render tests yourself, and confirm
a path containing markdown specials (e.g. `/Volumes/x/_todo_*pics`) renders literally, not as formatting.
