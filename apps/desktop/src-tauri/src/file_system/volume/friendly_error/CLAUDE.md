# Friendly error system

This module turns raw OS errors into warm, actionable messages so the user feels supported when something goes wrong.
It's one of Cmdr's UX differentiators: where other file managers show "I/O error: Operation timed out (os error 60)",
we show a friendly title, a plain-language explanation, and provider-specific advice ("This folder is managed by
**MacDroid**. Here's what to try: ...").

Parent: [`volume/CLAUDE.md`](../CLAUDE.md). The trait + manager + capability matrix live there; this file is just the
error-mapping subsystem.

For the broader error-handling conventions across the app, see `docs/guides/error-handling.md`.

## Philosophy

**The user should never feel alone with a broken state.** Every error message should feel like the app is putting its
hand on the user's shoulder and saying "Here's what happened, and here's what you can do." We go above and beyond: we
detect which cloud provider or mount tool manages the path, and tailor the suggestion to that specific app. A timeout on
a Dropbox folder gets different advice than a timeout on an SSHFS mount.

Power users also need the raw details (errno name, code) for debugging or bug reports. These are available in a
collapsible "Technical details" section, never hidden but never in your face either.

## Key files

- **`mod.rs`**: `FriendlyError`, `ErrorCategory`, `ErrorActionKind` data model + public re-exports
- **`errno.rs`**: Raw macOS errno → `FriendlyError` (37 codes), with a non-macOS fallback
- **`volume_error.rs`**: `VolumeError` → `FriendlyError` (dispatches to `errno` for raw `IoError`s)
- **`write_error.rs`**: `WriteOperationError` → `FriendlyError` (mirror of `volume_error` for the post-`map_volume_error` shape)
- **`empty_root.rs`**: TCC-restricted volume root hint (a single special case)
- **`kinds.rs`**: Shared "what kind of failure is this" classification used by both `volume_error` and `write_error`
- **`markdown.rs`**: `Markdown` newtype + `md!` macro: escapes interpolated runtime strings before they hit snarkdown
- **`provider.rs`**: `Provider` enum (19 variants), `detect_provider()`, `provider_suggestion()`, `enrich_with_provider()`

## Architecture

Three-layer mapping across two files, plus a third path for "succeeded but suspiciously empty":

**Layer 0**: typed git pass-through. `VolumeError::FriendlyGit(FriendlyGitError)` is a dedicated variant the git
module's volume hooks (`try_route_listing`, `try_route_metadata`, `try_open_blob_stream`) return when they detect a
git-shaped failure. `friendly_error_from_volume_error` matches it first and calls `to_friendly_error()` on the carried
payload, returning a fully-shaped `FriendlyError` with the right title, explanation, suggestion, and category, with no
errno mapping needed, no provider enrichment downstream. Keeps git-specific copy from getting clobbered by the generic
I/O fallback, end-to-end type-checked, no string parsing.

1. **`friendly_error_from_volume_error(err, path)`** (`volume_error.rs`): maps `VolumeError` variants and macOS errno
   codes (37 codes) to a `FriendlyError` with category (Transient/NeedsAction/Serious), title, explanation, suggestion,
   and raw detail.
2. **`enrich_with_provider(error, path)`** (`provider.rs`, re-exported from `mod.rs`): detects 19 cloud/mount providers
   from path patterns and `statfs` filesystem type, then overwrites the suggestion with provider-specific advice.
3. **`friendly_error_for_restricted_empty_root(volume_id, path)`** (`empty_root.rs`): for the case where the OS
   returns a successful empty listing at a volume root that's commonly hidden by macOS TCC (currently iCloud Drive
   without Full Disk Access). The streaming listing path (`file_system/listing/streaming.rs`) checks this after a
   successful empty read at the volume root and emits `listing-error` with the hint instead of `listing-complete`.
   Returns `None` for any other volume / non-root path so genuine empty directories don't get the warning.
4. **`friendly_from_write_error(err)`** (`write_error.rs`): variant-by-variant mapping from `WriteOperationError`
   (post-`map_volume_error`) to a `FriendlyError`. Used by `WriteErrorEvent::new` so every `write-error` event the FE
   receives carries a friendly payload, even on local-FS paths where the original `VolumeError` is no longer in scope.
   `TransferErrorDialog` renders this directly with category-based styling (mirrors the listing-error path's treatment).

The frontend receives the fully-baked `FriendlyError` struct via the `listing-error` and `write-error` Tauri events
and renders it with category-based visual styling. The frontend never sees errno codes or does OS-specific logic.

## Adding a new error message

When you need to handle a new errno or `VolumeError` variant:

1. Add the match arm in `friendly_error_from_volume_error` (`volume_error.rs`) or the corresponding errno arm in
   `errno.rs`.
2. Pick the right `ErrorCategory`: **Transient** (retry might work), **NeedsAction** (user must do something),
   **Serious** (something is genuinely broken)
3. Write the message following the rules below
4. Build `explanation` / `suggestion` with the `md!(...)` macro (see `markdown.rs`). Templates are trusted markdown;
   positional `{}` args route through `MarkdownArg::render_arg` which escapes plain strings (paths, OS messages, names)
   and passes a `Markdown` value through unescaped. **Use positional `{}` only** — captured-identifier syntax
   (`md!("foo {bar}")`) bypasses escaping and renders the literal `{bar}` in the UI.
5. Add a unit test asserting the category and that the text follows the style rules
6. Run the existing `error_messages_never_contain_error_or_failed` test to catch violations

## Adding a new provider

When a new cloud storage or mount tool becomes popular enough to detect:

1. Add a variant to the `Provider` enum in `provider.rs` with `display_name()` and `app_name()`
2. Add path detection in `detect_provider` (CloudStorage prefix, specific path, or `statfs` type)
3. Write provider-specific suggestions in `provider_suggestion` for each `ErrorCategory`
4. Add a unit test for path detection and suggestion content
5. Update `volumes/CLAUDE.md` provider table to keep the two lists in sync

## Writing rules for error messages

These are non-negotiable. The existing test suite enforces some of them automatically.

- **NEVER use "error" or "failed"** in titles, explanations, or suggestions. Say "Couldn't read" not "Read error". The
  automated test `error_messages_never_contain_error_or_failed` catches this.
- **Active voice, contractions**: "Cmdr couldn't..." not "The operation was unable to..."
- **No trivializing**: no "just", "simply", "easy", "all you have to do"
- **No permissive language**: "Check your connection" not "You might want to check..."
- **Direct and warm**: "Here's what to try:" not "Please attempt the following remediation steps:"
- **No em dashes**: use parentheses, commas, or new sentences
- **Sentence case in titles**: "Connection timed out" not "Connection Timed Out"
- **Bold key terms** with `**` only when it helps scanning (for example, provider names)
- **Platform-native terms**: "System Settings" on macOS, "Finder", "Trash"
- **Keep it short**: max two sentences for explanation, bullets for suggestions

Good example:
```
title: "Connection timed out"
explanation: "Cmdr tried to read this folder but the connection didn't respond in time."
suggestion: "Here's what to try:\n- Check that the device or server is reachable\n- ..."
```

Bad example (every rule violated):
```
title: "I/O Error: Operation Timed Out"   // "Error", Title Case
explanation: "An error occurred while the system attempted to access the directory."  // passive, "error"
suggestion: "You may want to try simply reconnecting the device."  // permissive, trivializing
```

## Provider detection strategies

- **`~/Library/CloudStorage/<Prefix>*`**: Dropbox, GoogleDrive, OneDrive, Box, pCloud, Nextcloud, SynologyDrive, Tresorit, ProtonDrive, Sync, Egnyte, MacDroid, plus a generic fallback for unrecognized providers
- **`~/Library/Mobile Documents/`**: iCloud Drive
- **`/Volumes/pCloudDrive`**: pCloud (FUSE virtual drive)
- **`/Volumes/veracrypt*`**: VeraCrypt
- **`~/.CMVolumes/`**: CloudMounter
- **`statfs` `f_fstypename` (macOS)**: macFUSE/SSHFS/Cryptomator/rclone (`macfuse`, `osxfuse`), pCloud (`pcloudfs`)

The `statfs` check runs only at error time (not on every listing), so the syscall cost is negligible.

## Key decisions

**Decision**: Friendly error mapping is two layers: errno mapping, then provider enrichment
**Why**: Not every provider+errno combination needs custom copy. The base errno message is always useful. Provider enrichment is additive, making the suggestion more specific when we recognize who manages the mount. Keeping them separate avoids a combinatorial explosion of messages.

**Decision**: Friendly error mapping lives in Rust, not the frontend
**Why**: The mapping needs access to the full path (for provider detection) and platform-specific errno codes. Doing it in Rust keeps the frontend thin (principle: smart backend, thin frontend) and avoids duplicating errno knowledge in TypeScript. The frontend receives a ready-to-render `FriendlyError` struct with markdown strings.

**Decision**: `FriendlyError.explanation` / `.suggestion` are typed `Markdown`, not `String`; built via the `md!` macro
**Why**: Raw OS messages and provider names contain markdown specials (`STATUS_DELETE_PENDING` rendered as italics because `format!()` baked the underscores straight into the explanation). The `Markdown` newtype + `md!` macro escape every interpolated runtime value via the `MarkdownArg` trait while leaving the trusted template literal alone. `#[serde(transparent)]` keeps the wire format identical to the old `String`, and the FE bindings.ts post-processing brands the type as `string & { readonly __markdown: unique symbol }` so the single `renderErrorMarkdown` call site only accepts wire-supplied markdown values. See `markdown.rs` for the macro, the trait, the HTML-entity escape strategy (snarkdown doesn't honor CommonMark `\` escapes, so `\_` would render literally — we emit `&#95;` instead, which snarkdown ignores and the browser decodes), the conservative escape set (line-start chars like `.` / `-` / `#` left alone so paths render naturally), and the captured-identifier footgun warning.

Full details: [DETAILS.md](DETAILS.md).
