# Error handling

How Cmdr turns raw OS errors into warm, actionable messages. Read this before adding new error states, providers, or
modifying error UI.

## Architecture

Two-layer pipeline in Rust, thin render layer in Svelte:

```
VolumeError (with errno)
  → friendly_error_from_volume_error()  → FriendlyError { category, title, explanation, suggestion, rawDetail }
  → enrich_with_provider()              → overwrites suggestion with provider-specific advice
  → Tauri emit("listing-error")         → frontend receives ready-to-render struct
  → ErrorPane.svelte                    → renders markdown, category icon, retry button
```

All classification and message authoring happens in Rust (`file_system/volume/friendly_error.rs`). The frontend renders
what it receives and never does OS-specific logic.

## Data model

```rust
pub struct FriendlyError {
    pub category: ErrorCategory,  // Transient, NeedsAction, or Serious
    pub title: String,            // Short heading, sentence case
    pub explanation: String,      // Markdown: what happened
    pub suggestion: String,       // Markdown: what to do
    pub raw_detail: String,       // For "Technical details" disclosure (for example, "ETIMEDOUT (os error 60)")
    pub retry_hint: bool,         // Shows "Try again" button when true
}
```

**Categories** determine the UI treatment:

| Category      | Icon                      | "Try again" button    | Meaning                                              |
| ------------- | ------------------------- | --------------------- | ---------------------------------------------------- |
| `Transient`   | Warning triangle (yellow) | Yes (if `retry_hint`) | Retry might work (timeouts, temp glitches)           |
| `NeedsAction` | None                      | No                    | User must do something (permissions, full disk)      |
| `Serious`     | Alert circle (red)        | No                    | Something is genuinely broken (hardware, corruption) |

## Error sources

### Layer 1: `VolumeError` variants

Each `VolumeError` variant maps to a `FriendlyError` in `friendly_error_from_volume_error()`:

| Variant                | Category    | Title                       | Retry hint |
| ---------------------- | ----------- | --------------------------- | ---------- |
| `NotFound`             | NeedsAction | "Path not found"            | No         |
| `PermissionDenied`     | NeedsAction | "No permission"             | No         |
| `AlreadyExists`        | NeedsAction | "Already exists"            | No         |
| `NotSupported`         | NeedsAction | "Not supported"             | No         |
| `DeviceDisconnected`   | NeedsAction | "Device disconnected"       | No         |
| `ReadOnly`             | NeedsAction | "Read-only"                 | No         |
| `StorageFull`          | NeedsAction | "Disk is full"              | No         |
| `ConnectionTimeout`    | Transient   | "Connection timed out"      | Yes        |
| `Cancelled`            | Transient   | "Cancelled"                 | Yes        |
| `IoError` (with errno) | Varies      | Varies                      | Varies     |
| `IoError` (no errno)   | Serious     | "Couldn't read this folder" | Yes        |

### Layer 2: macOS errno codes

`IoError` variants with a `raw_os_error` are matched against 37+ macOS errno codes in `friendly_error_from_errno()`.
Examples:

| Errno | Name         | Category    | Title                     |
| ----- | ------------ | ----------- | ------------------------- |
| 1     | EPERM        | NeedsAction | "Not permitted"           |
| 2     | ENOENT       | NeedsAction | "Path not found"          |
| 4     | EINTR        | Transient   | "Interrupted"             |
| 12    | ENOMEM       | Transient   | "Not enough memory"       |
| 13    | EACCES       | NeedsAction | "No permission"           |
| 16    | EBUSY        | Transient   | "Resource busy"           |
| 17    | EEXIST       | NeedsAction | "Already exists"          |
| 28    | ENOSPC       | NeedsAction | "Disk is full"            |
| 30    | EROFS        | NeedsAction | "Read-only volume"        |
| 35    | EAGAIN       | Transient   | "Temporarily unavailable" |
| 50    | ENETDOWN     | Transient   | "Network is down"         |
| 54    | ECONNRESET   | Transient   | "Connection reset"        |
| 60    | ETIMEDOUT    | Transient   | "Connection timed out"    |
| 61    | ECONNREFUSED | NeedsAction | "Connection refused"      |
| 62    | ELOOP        | NeedsAction | "Symlink loop"            |
| 63    | ENAMETOOLONG | NeedsAction | "Name too long"           |
| 69    | EDQUOT       | NeedsAction | "Quota exceeded"          |
| 80    | EAUTH        | NeedsAction | "Authentication required" |

Unrecognized errno codes fall through to a generic "Couldn't read this folder" message (Serious category, retry
enabled).

See the full list in `friendly_error.rs` — search for `fn friendly_error_from_errno`.

### Layer 3: provider enrichment

After the base error is built, `enrich_with_provider()` detects which cloud/mount provider manages the path and
overwrites the `suggestion` field with provider-specific advice.

**Detection strategies:**

| Strategy                           | Providers                                                                                                                                   |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `~/Library/CloudStorage/<Prefix>*` | Dropbox, GoogleDrive, OneDrive, Box, pCloud, Nextcloud, SynologyDrive, Tresorit, ProtonDrive, Sync, Egnyte, MacDroid, plus generic fallback |
| `~/Library/Mobile Documents/`      | iCloud Drive                                                                                                                                |
| `/Volumes/pCloudDrive`             | pCloud (FUSE)                                                                                                                               |
| `/Volumes/veracrypt*`              | VeraCrypt                                                                                                                                   |
| `~/.CMVolumes/`                    | CloudMounter                                                                                                                                |
| `statfs` `f_fstypename`            | macFUSE, SSHFS, Cryptomator, rclone (`macfuse`/`osxfuse`), pCloud (`pcloudfs`)                                                              |

The `statfs` check runs only at error time, so the syscall cost is negligible.

Each provider has suggestions per category. For example, MacDroid:

- **Transient**: "Unlock your phone and make sure file transfer is enabled in MacDroid"
- **NeedsAction**: "Open **MacDroid** and check that your phone is connected"
- **Serious**: "There's a problem with **MacDroid** or your device"

## Frontend flow

1. Backend emits `listing-error` Tauri event with `{ listingId, message, friendly? }`
2. `FilePane.svelte` checks `listingId` matches current load generation (discards stale events)
3. MTP volume? Short-circuit to `MtpConnectionView` (MTP has its own error UX)
4. Path gone? Auto-navigate to nearest valid parent (not an error)
5. Path exists but listing failed? Set `friendlyError` state, render `ErrorPane`

`ErrorPane.svelte` renders the pre-baked `FriendlyError` struct:

- Title with category-based icon
- Folder path in secondary text
- Explanation and suggestion as markdown (via `snarkdown`)
- "Try again" button for transient errors (tracks retry count and timestamps)
- "Open System Settings" button for permission-denied on macOS
- Collapsible "Technical details" with raw errno

## How to add a new error message

1. Add the match arm in `friendly_error_from_volume_error` (for `VolumeError` variants) or `friendly_error_from_errno`
   (for new errno codes)
2. Pick the right `ErrorCategory` (see the category table above)
3. Write the message following the [writing rules](#writing-rules)
4. Add a unit test asserting the category and that the text follows the style rules
5. Run the existing `error_messages_never_contain_error_or_failed` test to catch violations

## How to add a new provider

1. Add a variant to the `Provider` enum with `display_name()` and `app_name()`
2. Add path detection in `detect_provider` (CloudStorage prefix, specific path, or `statfs` type)
3. Write provider-specific suggestions in `provider_suggestion` for each `ErrorCategory`
4. Add a unit test for path detection and suggestion content
5. Update `volume/CLAUDE.md` provider table to keep the two lists in sync

## Writing rules

These are enforced by tests and code review.

- **Never use "error" or "failed"** in titles, explanations, or suggestions. Say "Couldn't read" not "Read error". The
  `error_messages_never_contain_error_or_failed` test catches this automatically.
- **Active voice, contractions**: "Cmdr couldn't..." not "The operation was unable to..."
- **No trivializing**: no "just", "simply", "easy", "all you have to do"
- **No permissive language**: "Check your connection" not "You might want to check..."
- **Direct and warm**: "Here's what to try:" not "Please attempt the following remediation steps:"
- **No em dashes**: use parentheses, commas, or new sentences
- **Sentence case in titles**: "Connection timed out" not "Connection Timed Out"
- **Bold key terms** with `**` only when it helps scanning (provider names, app names)
- **Platform-native terms**: "System Settings" on macOS, "Finder", "Trash"
- **Keep it short**: max two sentences for explanation, bullets for suggestions

**Good:**

```
title: "Connection timed out"
explanation: "Cmdr tried to read this folder but the connection didn't respond in time."
suggestion: "Here's what to try:\n- Check that the device or server is reachable\n- ..."
```

**Bad** (every rule violated):

```
title: "I/O Error: Operation Timed Out"       ← "Error", Title Case
explanation: "An error occurred while the system attempted to access the directory."  ← passive, "error"
suggestion: "You may want to try simply reconnecting the device."  ← permissive, trivializing
```

## Testing

- **Unit tests**: `friendly_error.rs` has tests for category assignment, writing rule enforcement, and provider
  detection
- **E2E tests**: `test/e2e-playwright/error-pane.spec.ts` injects errno codes via `inject_listing_error` (feature-gated
  behind `playwright-e2e`) and verifies the full render pipeline:
  - Transient error (ETIMEDOUT): title, markdown rendering, "Try again" button, retry clears error
  - NeedsAction error (EACCES): title, no "Try again" button, permission-specific suggestion
  - Accessibility: `role="alert"`, `<h2>` heading
- **Debug preview**: In dev builds, the debug window has an "Error pane preview" that calls `preview_friendly_error` to
  render any errno or VolumeError variant on either pane. Use this to visually verify new messages.

## Key files

| File                                                 | Role                                                              |
| ---------------------------------------------------- | ----------------------------------------------------------------- |
| `src-tauri/src/file_system/volume/friendly_error.rs` | All error classification, messages, and provider detection        |
| `src-tauri/src/file_system/volume/mod.rs`            | `VolumeError` enum definition                                     |
| `src-tauri/src/file_system/listing/streaming.rs`     | Emits `listing-error` events                                      |
| `src-tauri/src/commands/file_system.rs`              | `inject_listing_error` (E2E) and `preview_friendly_error` (debug) |
| `src/lib/file-explorer/pane/ErrorPane.svelte`        | Renders the error pane UI                                         |
| `src/lib/file-explorer/pane/error-pane-utils.ts`     | Markdown rendering helper                                         |
| `src/lib/file-explorer/types.ts`                     | `FriendlyError` and `ListingErrorEvent` TypeScript types          |
| `test/e2e-playwright/error-pane.spec.ts`             | E2E tests for error pane                                          |
