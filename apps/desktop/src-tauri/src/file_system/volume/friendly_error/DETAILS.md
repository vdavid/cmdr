# Friendly error system details

`CLAUDE.md` holds the must-knows. This file holds the depth: the mapping architecture, the writing rules in full, and
the step-by-step recipes for adding messages and providers.

## Philosophy

The user should never feel alone with a broken state. Every message should feel like the app putting its hand on the
user's shoulder: "Here's what happened, and here's what you can do." We detect which cloud provider or mount tool manages
the path and tailor the suggestion to that app, so a timeout on a Dropbox folder gets different advice than a timeout on
an SSHFS mount. Power users still get the raw errno name and code in a collapsible "Technical details" section: never
hidden, never in your face.

## Architecture

Three-layer mapping across two files, plus a path for "succeeded but suspiciously empty", plus a typed git pass-through:

- **Layer 0: typed git pass-through.** `VolumeError::FriendlyGit(FriendlyGitError)` is a dedicated variant the git
  module's volume hooks (`try_route_listing`, `try_route_metadata`, `try_open_blob_stream`) return when they detect a
  git-shaped failure. `friendly_error_from_volume_error` matches it first and calls `to_friendly_error()` on the carried
  payload, returning a fully-shaped `FriendlyError` with no errno mapping and no provider enrichment downstream. Keeps
  git copy from being clobbered by the generic I/O fallback, end-to-end type-checked, no string parsing.
- **Layer 1: `friendly_error_from_volume_error(err, path)`** (`volume_error.rs`): maps `VolumeError` variants and macOS
  errno codes (37 codes) to a `FriendlyError` with category (Transient/NeedsAction/Serious), title, explanation,
  suggestion, and raw detail.
- **Layer 2: `enrich_with_provider(error, path)`** (`provider.rs`, re-exported from `mod.rs`): detects 18 cloud/mount
  providers from path patterns and `statfs` filesystem type, then overwrites the suggestion with provider-specific
  advice.
- **Empty-root path: `friendly_error_for_restricted_empty_root(volume_id, path)`** (`empty_root.rs`): for an OS-returned
  successful empty listing at a volume root commonly hidden by macOS TCC (currently iCloud Drive without Full Disk
  Access). The streaming listing path (`file_system/listing/streaming.rs`) checks this after a successful empty read at
  the volume root and emits `listing-error` with the hint instead of `listing-complete`. Returns `None` for any other
  volume / non-root path so genuine empty directories don't warn.
- **Write path: `friendly_from_write_error(err)`** (`write_error.rs`): variant-by-variant mapping from
  `WriteOperationError` (post-`map_volume_error`) to a `FriendlyError`. Used by `WriteErrorEvent::new` so every
  `write-error` event carries a friendly payload, even on local-FS paths where the original `VolumeError` is out of
  scope. `TransferErrorDialog` renders this with category-based styling, mirroring the listing-error path.

The frontend receives the fully-baked `FriendlyError` struct via the `listing-error` and `write-error` Tauri events and
renders it with category-based visual styling.

## Adding a new error message

For a new errno or `VolumeError` variant:

1. Add the match arm in `friendly_error_from_volume_error` (`volume_error.rs`) or the corresponding errno arm in
   `errno.rs`.
2. Pick the `ErrorCategory`: Transient (retry might work), NeedsAction (user must do something), Serious (genuinely
   broken).
3. Write the message following the rules below.
4. Build `explanation` / `suggestion` with the `md!(...)` macro. Templates are trusted markdown; positional `{}` args
   route through `MarkdownArg::render_arg` which escapes plain strings and passes a `Markdown` value through unescaped.
   Use positional `{}` only.
5. Add a unit test asserting the category and that the text follows the style rules.
6. Run `error_messages_never_contain_error_or_failed` to catch violations.

## Adding a new provider

When a new cloud storage or mount tool becomes popular enough to detect:

1. Add a variant to the `Provider` enum in `provider.rs` with `display_name()` and `app_name()`.
2. Add path detection in `detect_provider` (CloudStorage prefix, specific path, or `statfs` type).
3. Write provider-specific suggestions in `provider_suggestion` for each `ErrorCategory`.
4. Add a unit test for path detection and suggestion content.
5. Update the `volumes/CLAUDE.md` provider table to keep the two lists in sync.

## Provider detection strategies

- **`~/Library/CloudStorage/<Prefix>*`**: Dropbox, GoogleDrive, OneDrive, Box, pCloud, Nextcloud, SynologyDrive,
  Tresorit, ProtonDrive, Sync, Egnyte, MacDroid, plus a generic fallback for unrecognized providers.
- **`~/Library/Mobile Documents/`**: iCloud Drive.
- **`/Volumes/pCloudDrive`**: pCloud (FUSE virtual drive).
- **`/Volumes/veracrypt*`**: VeraCrypt.
- **`~/.CMVolumes/`**: CloudMounter.
- **`statfs` `f_fstypename` (macOS)**: macFUSE/SSHFS/Cryptomator/rclone (`macfuse`, `osxfuse`), pCloud (`pcloudfs`).

The `statfs` check runs only at error time (not on every listing), so the syscall cost is negligible.

## Writing rules for error messages

Non-negotiable. The test suite enforces some automatically.

- **Never use "error" or "failed"** in titles, explanations, or suggestions. Say "Couldn't read" not "Read error". The
  `error_messages_never_contain_error_or_failed` test catches this.
- **Active voice, contractions**: "Cmdr couldn't..." not "The operation was unable to..."
- **No trivializing**: no "just", "simply", "easy", "all you have to do".
- **No permissive language**: "Check your connection" not "You might want to check..."
- **Direct and warm**: "Here's what to try:" not "Please attempt the following remediation steps:"
- **No em dashes**: use parentheses, commas, or new sentences.
- **Sentence case in titles**: "Connection timed out" not "Connection Timed Out".
- **Bold key terms** with `**` only when it helps scanning (for example, provider names).
- **Platform-native terms**: "System Settings" on macOS, "Finder", "Trash".
- **Keep it short**: max two sentences for explanation, bullets for suggestions.

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

## Markdown escaping

`FriendlyError.explanation` / `.suggestion` are typed `Markdown`, not `String`, built via the `md!` macro.

- **Why typed.** Raw OS messages and provider names contain markdown specials (`STATUS_DELETE_PENDING` rendered as
  italics because `format!()` baked the underscores into the explanation). The `Markdown` newtype + `md!` macro escape
  every interpolated runtime value via the `MarkdownArg` trait while leaving the trusted template literal alone.
- **Wire format.** `#[serde(transparent)]` keeps the wire format identical to a plain `String`. The `bindings.ts`
  post-processing brands the type as `string & { readonly __markdown: unique symbol }` so the single
  `renderErrorMarkdown` call site only accepts wire-supplied markdown values.
- **Escape strategy.** snarkdown doesn't honor CommonMark `\` escapes, so `\_` would render literally; we emit `&#95;`
  instead, which snarkdown ignores and the browser decodes. The escape set is conservative: line-start chars like `.` /
  `-` / `#` are left alone so paths render naturally. See `markdown.rs` for the macro, the trait, and the
  captured-identifier footgun warning.

## Key decisions

- **Two layers (errno mapping, then provider enrichment).** Not every provider+errno combination needs custom copy; the
  base errno message is always useful, and provider enrichment is additive. Keeping them separate avoids a combinatorial
  explosion of messages.
- **Mapping in Rust, not the frontend.** The mapping needs the full path (for provider detection) and platform-specific
  errno codes. Doing it in Rust keeps the frontend thin and avoids duplicating errno knowledge in TypeScript.
