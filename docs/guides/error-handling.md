# Error handling

How Cmdr turns a raw OS failure into a warm, actionable message. Read this before adding an error state, a provider, or
error UI. It's the map: each layer's mechanics live in the `C+D.md` next to its code, linked below.

## The split: Rust classifies, the frontend words

One idea carries the whole design. **The backend ships a typed, word-free classification; the frontend owns 100% of the
prose.**

```
raw failure (VolumeError, errno, git, empty root)
  → Rust: listing_error_from_volume_error()   → a semantic reason + typed params
  → Rust: enrich_with_provider()              → sets the detected provider (never prose)
  → emit("listing-error")                     → ListingError { category, reason, provider, actionKind, retryHint, rawDetail }
  → FE: renderListingError()                  → picks the message factory, applies the provider override
  → ErrorPane.svelte                          → icon, markdown, action row, technical details
```

Why the split: the words have to be translated (ten locales), and translation belongs to the catalog pipeline, not to
Rust string literals. It also keeps classification testable without asserting on prose.

The consequence that bites: **reason, provider, and git-kind names are an IPC contract.** Rename a `ListingErrorReason`
variant, a `Provider` variant, or a `FriendlyGitErrorKind` on one side only and the parity test fails (or, worse, it
mis-renders at runtime). Change both sides in the same commit.

## Three error paths

- **Listing errors** (a pane can't show a folder): the pipeline above, ending in `ErrorPane`.
- **Write errors** (a copy, move, delete, or compress didn't finish): the backend emits `write-error` carrying a typed
  `WriteOperationError`, and the frontend renders it through `file-operations/transfer/transfer-error-messages.ts` into
  `TransferErrorDialog` / `FallbackErrorContent`. Same principle, separate factories; they share the
  `FriendlyErrorMessage` shape so the two can converge later.
- **Mutation refusals** (a rename, New Folder, New File, or single move to the Trash didn't happen): the command RETURNS
  a typed `MutationError` (no event), and the frontend renders one plain-text line through
  `apps/desktop/src/lib/file-operations/mutation-error-messages.ts`, inline under the name field or in a toast.
  `MutationError::Volume` carries the WHOLE `VolumeError`, which is itself the wire type (adjacently tagged, structured
  fields intact), so a backend that grows a variant reaches the frontend without a second vocabulary to keep in step.
  `renderVolumeError` is exported for any other surface that gets one.

## Where each piece lives

- **Rust classification** (errno → reason, provider detection, category / retry / action):
  `crates/cmdr-fs/src/volume/friendly_error/CLAUDE.md`. Its `DETAILS.md` holds the provider-detection strategies, the
  permission-denied three-way, and the recipe for a new reason.
- **Frontend copy** (the factories, the catalog, the escaping boundary):
  `apps/desktop/src/lib/error-messages/CLAUDE.md`.
- **The English strings themselves**: `apps/desktop/src/lib/intl/messages/en/errors.json`. Editing copy means editing
  the catalog and running `pnpm intl:keys`, never touching the `.ts` factories.
- **The error screen's buttons** (which way out renders, and why each gate exists):
  `apps/desktop/src/lib/file-explorer/pane/DETAILS.md` § The error screen's ways out.
- **Emitting the event**: `apps/desktop/src-tauri/src/file_system/listing/streaming.rs`.
- **Mutation refusals**: the enum is `apps/desktop/src-tauri/src/file_system/write_operations/mutation_error.rs`; the
  words are `apps/desktop/src/lib/file-operations/mutation-error-messages.ts`, and
  `apps/desktop/src/lib/file-operations/mutation-error.ts` is what keeps the typed value alive across the throw
  (`throwIpcError` would flatten it into a JSON string).

## Cross-cutting rules

These sit between the layers, so neither side's doc owns them alone.

- **Never classify by string-matching a message.** Switch on the typed reason, the errno, or `actionKind`. This is a
  project hard rule (`AGENTS.md` § Hard rules), enforced by `error-string-match` and `cmdr/no-error-string-match`.
- **`VolumeError`'s `Display` is for LOGS AND DEBUG ONLY**, and its doc comment says so. Every user-facing word comes
  from the typed variant. There is deliberately no `From<std::io::Error> for VolumeError`: the three path-carrying
  variants are defined to carry the PATH, which a bare `io::Error` doesn't hold, so io errors convert through
  `VolumeError::from_io_at(&err, path)` (or `from_io_without_path` where none exists).
  `conformance::assert_not_found_carries_the_path` holds every backend to it.
- **A typed IPC error needs a typed TIMEOUT too.** `commands/util.rs`'s `timeout_detached_typed` /
  `blocking_typed_result_with_timeout` mint the caller's own error type on the deadline, so the frontend matches ONE
  exhaustive union instead of a typed error plus a stringly-typed timeout beside it. A `MutationError::TimedOut` also
  means the write may STILL LAND (the deadline detaches, it doesn't cancel), and the copy says so.
- **`category` picks the icon and severity color; it does NOT gate the buttons.** `retryHint` alone decides "Try again",
  and it's deliberately set under all three categories. `actionKind` alone decides "Open System Settings".
- **Every interpolated runtime value passes through `esc(...)`.** The composed explanation and suggestion are
  `{@html}`-injected through snarkdown, so a path, an OS message, or a device name that skips escaping is an XSS hole.
  Template literals are the only trusted markdown.
- **`rawDetail` is plain text, never markdown**: it renders verbatim in the technical-details disclosure.

## Writing rules for error copy

Enforced by `friendly-error-style.test.ts` over every reason, every provider × category, and every git kind. General
voice rules live in `docs/style-guide.md`; these are the error-specific ones.

- **Never the words "error" or "failed"** in a title, explanation, or suggestion. "Couldn't read this folder", not "Read
  error".
- **Active voice with contractions**: "Cmdr couldn't reach the server", not "The operation was unable to complete".
- **No trivializing**: no "just", "simply", "easy", "all you have to do".
- **No permissive hedging**: "Check your connection", not "You might want to check your connection".
- **Sentence case titles**: "Connection timed out", not "Connection Timed Out".
- **Platform-native terms**: "System Settings", "Finder", "Trash".
- **Bold key terms** with `**` only where it helps scanning (provider and app names).
- **Short**: at most two sentences of explanation; suggestions as bullets.

Good:

```
title:       "Connection timed out"
explanation: "Cmdr tried to read this folder but the connection didn't respond in time."
suggestion:  "Here's what to try:\n- Check that the device or server is reachable\n- ..."
```

Bad, every rule broken:

```
title:       "I/O Error: Operation Timed Out"                                        ← "Error", Title Case
explanation: "An error occurred while the system attempted to access the directory." ← passive, "error"
suggestion:  "You may want to try simply reconnecting the device."                   ← hedging, trivializing
```

## Adding an error message or a provider

Both sides change together, in one commit. The per-side recipes are canonical in the two `CLAUDE.md`s:

1. **Rust**: add the `ListingErrorReason` variant with its typed params, map it in `errno.rs` / `volume_error.rs` /
   `kinds.rs` picking category, retry hint, and action kind, and add a typed-mapping test.
2. **Frontend**: add the `errors.<reason>.{title,explanation,suggestion}` keys to `errors.json` with `@key`
   descriptions, run `pnpm intl:keys`, translate into every locale ([guide](i18n-translation.md)), extend the factory
   union, and add the reason to the STYLE matrix.

A new provider additionally needs its detection arm in `detect_provider`, its suggestions in
`provider-error-messages.ts`, and a row in the `volumes/CLAUDE.md` provider table.

A new **mutation refusal** is the same two-sided move in a different pair of files: add the `MutationError` variant with
its typed fields, return it from `create.rs` / `rename.rs`, add the `errors.mutation.<variant>` key with its `@key`
description, run `pnpm intl:keys` + `node apps/desktop/scripts/sync-locale-keys.ts`, translate into every locale, and
add the arm to `MUTATION_MESSAGE` in `mutation-error-messages.ts`. The record type there demands every variant, so the
frontend can't compile with one missing; `mutation-error-messages.test.ts` catches the catalog key you forgot.

Note for a new reason: it gets **no** golden-fixture entry. The frozen fixture pins pre-existing output, so there's
nothing for a new reason to be pinned against.

## Testing

- **Rust**: typed-mapping tests in `friendly_error/tests.rs` drive the public entry points, so they cover every sibling
  module at once.
- **Frontend parity**: `friendly-error-parity.test.ts` asserts the factories reproduce
  `__fixtures__/friendly_error_golden.json` byte-for-byte. If it fails, the copy drifted: fix the factory, don't
  regenerate the fixture.
- **Frontend style**: `friendly-error-style.test.ts` enforces the writing rules above.
- **E2E**: `apps/desktop/test/e2e-playwright/error-pane.spec.ts` injects errno codes through `inject_listing_error`
  (feature-gated behind `playwright-e2e`) and checks the rendered pane, including the per-platform action row.
- **Debug preview**: dev builds carry an "Error pane preview" panel that calls `preview_friendly_error` to render any
  errno or `VolumeError` variant on either pane. Use it to eyeball new copy.
