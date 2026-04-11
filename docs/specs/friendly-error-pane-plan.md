# Friendly error pane: Rich, provider-aware filesystem error display

Replace the raw red "I/O error: Operation timed out (os error 60)" text with a structured, warm, and actionable error
pane that recognizes the user's cloud provider, explains what went wrong in plain language, and offers concrete next
steps.

## Why

- The current error display is a single `<div class="error-message">{error}</div>` showing the raw Rust `Display`
  output. It's functional but scary, ugly, and unhelpful — violates every principle in our style guide ("never use the
  words 'error' or 'failed'", "suggest a next step", "conversational, positive, actionable").
- The `PermissionDeniedPane` already proves the pattern: a dedicated component with friendly copy, clear instructions,
  and an actionable CTA. But it's a one-off. Every other error falls through to the raw string.
- We already have `parse_cloud_provider_name()` in `src-tauri/src/volumes/mod.rs` (the volume discovery module, separate
  from `file_system/volume/`) that detects 5 cloud providers from path patterns. The new provider detection for error
  enrichment lives in `file_system/volume/friendly_error.rs` and is independent — it doesn't extend
  `parse_cloud_provider_name` directly (different module, different purpose: display names vs error suggestions) but
  reuses the same path-prefix matching strategy.
- macOS gives us the raw errno via `std::io::Error::raw_os_error()`. These are stable POSIX codes we can map to friendly
  messages.

## Design

### Data model (Rust)

New struct `FriendlyError` in a new file `file_system/volume/friendly_error.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendlyError {
    pub category: ErrorCategory,
    pub title: String,
    pub explanation: String,       // Markdown (rendered by snarkdown on FE)
    pub suggestion: String,        // Markdown (can contain /settings/... links)
    pub raw_detail: String,        // "ETIMEDOUT (os error 60)" — for technical details disclosure
    pub retry_hint: bool,          // FE shows a "Try again" button
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Transient,   // Might work if you retry (timeouts, temp resource issues)
    NeedsAction, // User must do something (permission denied, disk full, device disconnected)
    Serious,     // Something is genuinely broken (I/O error, bad fd, corrupted data)
}
```

**Why `category`**: The frontend uses it to select visual tone (amber vs neutral vs red) and whether to show a retry
button. This is OS-agnostic — the same three categories will work on Linux and Windows later. The Rust mapping produces
the category from OS-specific errno codes, so the FE never sees raw errnos.

**Why markdown in `explanation` and `suggestion`**: Enables bold, italic, bullets, and links (including `/settings/...`
internal links the FE can resolve). The content is always our own hardcoded strings, never user input, so XSS isn't a
concern. We use `snarkdown` (2.0.0, ~1 KB, has TS types) for rendering.

### Two-layer mapping (Rust)

**Layer 1 — errno → FriendlyError (static match):**

A `fn friendly_error_from_volume_error(err: &VolumeError, path: &Path) -> FriendlyError` function that:

1. For `VolumeError::IoError { raw_os_error: Some(errno), .. }`: matches errno against ~36 macOS codes
2. For other `VolumeError` variants: maps directly (for example, `PermissionDenied` → NeedsAction, `ConnectionTimeout` →
   Transient, `StorageFull` → NeedsAction, `DeviceDisconnected` → NeedsAction)
3. Falls back to a sensible default for `IoError { raw_os_error: None, .. }` and unknown codes

This is `#[cfg(target_os = "macos")]` for the errno match body — Linux and Windows get their own mapping files later.
The `VolumeError` variant matching is OS-agnostic.

**How errno is preserved**: The current `VolumeError::IoError(String)` variant changes to
`VolumeError::IoError { message: String, raw_os_error: Option<i32> }`. The `From<std::io::Error>` impl (line 205)
captures `err.raw_os_error()` alongside the string message. This is a minimal change — `VolumeError` already stores
strings (by design: `std::io::Error` is not `Clone`), and `Option<i32>` adds negligible overhead. The `Display` impl for
`IoError` stays the same (uses `message` only). All existing callers that construct `VolumeError::IoError("...")` change
to `VolumeError::IoError { message: "...".into(), raw_os_error: None }`.

**Where to call it**: At the emission site in `streaming.rs` (line 187, the `Ok(Err(e))` branch). The error propagation
needs to change: instead of `entries_result.map_err(|e| std::io::Error::other(e.to_string()))` at line 320 (which wraps
`VolumeError` in a new `io::Error`, losing `raw_os_error`), propagate the `VolumeError` directly through the task. The
background task's return type changes from `Result<(), std::io::Error>` to `Result<(), VolumeError>`. Then
`friendly_error_from_volume_error` is called at the emission site with the original `VolumeError` that still has
`raw_os_error`.

**SMB and other volume types**: `SmbVolume` produces `VolumeError::ConnectionTimeout`, `DeviceDisconnected`, etc. These
are already meaningful variants that `friendly_error_from_volume_error` maps to the right category without needing POSIX
errnos. MTP volumes are carved out (the MTP error path in `FilePane.svelte` short-circuits to `MtpConnectionView` before
`ErrorPane` renders).

**Why not map in the frontend**: The frontend shouldn't know about OS-specific errno codes. This keeps the FE thin and
the mapping colocated with the filesystem code that produces the errors.

**Layer 2 — path-based provider enrichment:**

A `fn enrich_with_provider(error: &mut FriendlyError, path: &Path)` function that:

1. Detects the cloud/mount provider from the path (extending existing `parse_cloud_provider_name` patterns)
2. Overwrites `suggestion` (and sometimes `explanation`) with provider-specific advice
3. Leaves `title`, `category`, and `retry_hint` unchanged (those come from the errno, not the provider)

Provider detection covers 19 providers across three path-based strategies:

- `~/Library/CloudStorage/<Prefix>*` — 13 providers (Dropbox, GoogleDrive, OneDrive, Box, pCloud, Nextcloud,
  SynologyDrive, Tresorit, ProtonDrive, Sync, Egnyte, MacDroid, plus a generic "unknown cloud provider" fallback for
  ExpanDrive/Mountain Duck/etc.)
- `~/Library/Mobile Documents/` — iCloud Drive
- Specific paths: `/Volumes/pCloudDrive`, `/Volumes/veracrypt*`, `~/.CMVolumes/*`
- `statfs` fstype: `macfuse`/`osxfuse`/`pcloudfs` for FUSE mounts (including SSHFS, Cryptomator, rclone)

**Why two layers**: Not every provider+errno combination needs custom copy. The base errno message is always useful. The
provider enrichment is additive — it makes the suggestion more specific when we know who manages the mount.

### Event payload (Rust → Frontend)

`ListingErrorEvent` changes from:

```rust
pub struct ListingErrorEvent {
    pub listing_id: String,
    pub message: String,
}
```

to:

```rust
pub struct ListingErrorEvent {
    pub listing_id: String,
    pub message: String,                    // Kept for backwards compat (plain text summary)
    pub friendly: Option<FriendlyError>,    // New: structured error info when available
}
```

**Why `Option`**: Not all error paths produce `FriendlyError` (for example, task panics, non-I/O errors). The FE falls
back to `message` when `friendly` is `None`. This also makes the migration incremental — we can wire up `friendly` for
listing errors first, then expand to write operations later.

**MTP error path**: `FilePane.svelte` lines 831–837 short-circuit on MTP listing errors by calling `onMtpFatalError()`.
This path must be preserved — `ErrorPane` does NOT render for MTP errors. The MTP check (`if (isMtpView)`) stays as-is,
before the `ErrorPane` rendering logic. MTP volumes have their own `MtpConnectionView` fallback which is the right UX
for device disconnection. The `ErrorPane` is for local/cloud/FUSE volumes only.

**Replacing `isPermissionDenied`**: The current string-based check
(`error.includes('Permission denied') || error.includes('os error 13')`) is replaced by checking `friendly?.category`.
When `friendly` is present, use `friendly.category` to determine rendering. When `friendly` is `None` (fallback),
display the raw `message` string in the old `<div class="error-message">` style. The `isPermissionDenied` derived state
and its string matching are removed entirely.

**Deleted-path auto-navigation**: `FilePane.svelte` lines 840–854 check `pathExists()` when a listing error arrives. If
the path was deleted, it auto-navigates to the nearest valid parent instead of showing an error. This logic must be
preserved exactly as-is — `ErrorPane` only renders in the "path exists but has another error" branch. The `pathExists`
check runs before any error display decision. The flow is:

1. `listing-error` arrives → check if MTP (short-circuit to `MtpConnectionView`)
2. → check `pathExists(loadPath)` → if gone, auto-navigate to valid parent
3. → if path exists, set error state → render `ErrorPane` (if `friendly` present) or raw message div (if not)

**"Task failed" cleanup**: The `Err(e)` branch in `streaming.rs` (task panics) currently emits "Task failed: {e}". This
violates the style guide. Change it to "Something went wrong while reading this folder" with `friendly: None` (task
panics are rare internal errors, not filesystem errors — no `FriendlyError` enrichment is appropriate).

### Frontend component: `ErrorPane.svelte`

Replaces both `<div class="error-message">` and `PermissionDeniedPane`. Located at
`src/lib/file-explorer/pane/ErrorPane.svelte`.

**No animations**: The current `PermissionDeniedPane` has a Lottie lock animation. We intentionally drop it — David
decided against animations for V1 of the unified error pane. Illustrations may be added later. The component has no icon
or animation placeholder in its initial version.

**Layout:**

```
┌─────────────────────────────────────┐
│    Connection timed out             │  ← title (--font-size-xl, 600 weight)
│                                     │
│    /path/to/folder                  │  ← path (--color-text-secondary)
│                                     │
│    Cmdr tried to read this folder   │  ← explanation (markdown → HTML)
│    but the device didn't respond    │
│    in time.                         │
│                                     │
│    This folder is managed by        │  ← suggestion (markdown → HTML)
│    **MacDroid**. Here's what to     │
│    try:                             │
│    • Open MacDroid and check your   │
│      phone is connected             │
│    • Unplug and replug the USB      │
│      cable, then navigate here      │
│      again                          │
│                                     │
│         [ Try again ]               │  ← only for Transient category
│                                     │
│    ▸ Technical details              │  ← collapsible, shows raw_detail
│      ETIMEDOUT (os error 60)        │
│      Retry #2 · first try 45s ago   │
│      · last try 12s ago             │
│                                     │
└─────────────────────────────────────┘
```

**Visual tone by category:**

- **Transient**: `--color-warning` title, "Try again" button (accent color)
- **NeedsAction**: `--color-text-primary` title, action-specific CTA if applicable (for example, "Open System Settings"
  for permission denied — reuse existing `openPrivacySettings()`)
- **Serious**: `--color-error` title, no retry button

**Retry logic (Transient errors only):**

"Try again" re-navigates to the current path (calls the existing `navigateTo` function). The component tracks:

- `retryCount: number` — incremented on each retry
- `retryTimestamps: number[]` — timestamps of each attempt (for relative time display)
- Displays: "Retry #N · first try Xs ago · last try Ys ago" in the technical details section
- The retry state resets when the user navigates away (component is destroyed/recreated per path)

**Why re-navigate**: For listing errors, the failed operation is always "read this directory". Re-navigating is the
natural retry. For write operations (future), retry semantics would differ, but that's out of scope here.

**Markdown rendering:**

`snarkdown` (2.0.0) renders markdown to HTML. A thin wrapper function `renderErrorMarkdown(md: string): string` calls
`snarkdown(md)` and returns the HTML string. The input is always our own hardcoded strings from Rust, never user input.

Used via Svelte's `{@html renderErrorMarkdown(friendly.suggestion)}`.

**Why snarkdown**: ~1 KB, has TypeScript types, supports our needed subset (bold, italic, bullets, links). The input is
always our own hardcoded strings from Rust, never user-generated content.

### E2E testability

**The challenge**: `InMemoryVolume::list_directory` returns `VolumeError` (not `std::io::Error`), and its code path
doesn't go through `read_directory_with_progress` in `streaming.rs`. But the `friendly_error_from_volume_error`
conversion happens in `streaming.rs` where `std::io::Error` is available. We need the injected error to surface as a
`std::io::Error` with a real `raw_os_error()` at the streaming level.

**Approach**: Add an `injected_error` field to `InMemoryVolume` behind `#[cfg(feature = "playwright-e2e")]`:

```rust
#[cfg(feature = "playwright-e2e")]
injected_error: std::sync::Mutex<Option<i32>>,  // raw errno to inject
```

When set, `InMemoryVolume::list_directory_with_progress` returns
`Err(VolumeError::IoError { message: "...", raw_os_error: Some(errno) })`. Since the background task now propagates
`VolumeError` directly (not wrapped in `std::io::Error`), this flows through to `streaming.rs` where
`friendly_error_from_volume_error` is called, producing a real `FriendlyError` with the correct category, title, and
suggestion.

Add a Tauri command `inject_listing_error(volume_id: String, error_code: i32)` (feature-gated) that sets this field.

A Playwright test:

1. Navigates to the in-memory volume
2. Calls `inject_listing_error` with errno 60 (ETIMEDOUT)
3. Navigates into a subdirectory to trigger the error
4. Asserts: title text present, suggestion rendered as HTML (not raw markdown), retry button visible, technical details
   collapsible works
5. Clicks "Try again", verifies retry count increments
6. Tests a11y: `role="alert"` on the error pane, proper heading hierarchy, sufficient color contrast
7. Tests a NeedsAction case (errno 13, EACCES): verifies no retry button, permission-specific suggestion

### Writing style

All error messages follow a checklist distilled from the project style guide and David's personal writing style:

1. **Never use "error" or "failed"** — style guide rule. "Couldn't read" not "Read error".
2. **Active voice, contractions** — "Cmdr couldn't..." not "The operation was..."
3. **No trivializing** — no "just reconnect", no "simply retry"
4. **No permissive language** — "Check your connection" not "You might want to check..."
5. **Direct and warm** — like a friend helping. "Here's what to try:" not "Please attempt the following:"
6. **No em dashes** — use parentheses, commas, or new sentences instead
7. **Sentence case everywhere** — "Connection timed out" not "Connection Timed Out"
8. **Bold key terms** only when it helps scanning
9. **Technical terms in backticks** — "`ETIMEDOUT`" in the technical details
10. **Keep it short** — two sentences max for explanation, bullets for suggestions
11. **Link the destination, not the sentence** — "Open [Privacy & Security settings](/settings/privacy)"
12. **No "error" in the title** — use the human consequence: "Connection timed out", "No permission", "Disk is full"
13. **Platform-native terms** — "System Settings" on macOS (not "Settings"), "Finder" (not "file manager")

## Milestones

### Milestone 1: Rust data model + errno mapping + provider detection + writing

**Files to create:**

- `src-tauri/src/file_system/volume/friendly_error.rs` — `FriendlyError` struct, `ErrorCategory` enum,
  `friendly_error_from_volume_error()`, `enrich_with_provider()`

**Files to modify:**

- `src-tauri/src/file_system/volume/mod.rs` — add `pub mod friendly_error;`. Change `VolumeError::IoError(String)` to
  `VolumeError::IoError { message: String, raw_os_error: Option<i32> }`. Update `From<std::io::Error>` to capture
  `err.raw_os_error()`. Update `Display` impl. Update all call sites that construct `IoError` directly — grep for
  `IoError(` across the crate; expect hits in `local_posix.rs`, `mtp.rs`, `smb.rs`, `in_memory.rs`,
  `volume_strategy.rs`, `volume_copy.rs`, and their test files. Each becomes
  `IoError { message: "...".into(), raw_os_error: None }` (or `Some(code)` where an errno is available).
- `src-tauri/src/file_system/listing/streaming.rs` — change the background task return type from
  `Result<(), std::io::Error>` to `Result<(), VolumeError>`. Specific changes:
  - Line 263: change `std::io::Error::new(NotFound, ...)` to `VolumeError::NotFound(...)`
  - Line 312–315: change `std::io::Error::other("Directory listing thread terminated...")` to
    `VolumeError::IoError { message: "...", raw_os_error: None }`
  - Line 320: remove `.map_err(|e| std::io::Error::other(e.to_string()))` — `VolumeError` propagates directly
  - `ListingErrorEvent` gains `friendly: Option<FriendlyError>`. In the `Ok(Err(e))` branch (line 187), call
    `friendly_error_from_volume_error(&e, &path)` and `enrich_with_provider(&mut friendly, &path)` before emitting. The
    path is available from `STREAMING_STATE`. In the `Err(e)` branch (task panic, line 177), use
    `message: "Something went wrong while reading this folder"` with `friendly: None`.
  - `ListingStatus::Error { message: String }` (line 34): leave as-is. This enum is only used in the initial
    `StreamingListingStartResult` response, not for the async error event. The `FriendlyError` travels via the
    `listing-error` Tauri event, not through `ListingStatus`.

**Writing pass (included in M1, not deferred):**

- Write all ~36 errno messages and ~19 provider suggestions as part of the Rust code in this milestone
- Review the first 3–5 samples against the style checklist before writing the rest
- David will verify a sample error rendering after M2 and we can tweak the copy then, but the initial writing quality
  should be high enough to ship

**Tests:**

- Unit tests in `friendly_error.rs`: test each errno category, test provider detection for all 19 providers, test
  enrichment overwrites suggestion but not title
- Run `./scripts/check.sh --check clippy --check rustfmt` after

**Why this milestone first**: The Rust code is self-contained and testable without any frontend. Getting the data model
and writing right before touching the FE avoids rework.

### Milestone 2: Frontend `ErrorPane` component

**Files to create:**

- `src/lib/file-explorer/pane/ErrorPane.svelte` — the new unified error component

**Files to modify:**

- `package.json` — add `snarkdown` dependency
- `src/lib/file-explorer/types.ts` — update `ListingErrorEvent` to include `friendly?: FriendlyError`
- `src/lib/file-explorer/pane/FilePane.svelte` — replace both `isPermissionDenied` check + `PermissionDeniedPane` and
  the `<div class="error-message">` with the new `ErrorPane`

**Files to delete:**

- `src/lib/file-explorer/pane/PermissionDeniedPane.svelte` — absorbed into `ErrorPane`

**Tests:**

- Vitest unit test for `renderErrorMarkdown()` (markdown rendering + internal link post-processing)
- Run `./scripts/check.sh --check svelte-check --check eslint --check stylelint` after

**Why milestone 2 depends on 1**: The FE component needs to know the `FriendlyError` shape to render it. The TypeScript
interface mirrors the Rust struct.

### Milestone 3: E2E test + a11y verification

**Files to modify:**

- `src-tauri/src/file_system/volume/in_memory.rs` — add `injected_error` field behind feature flag
- `src-tauri/src/commands/file_system.rs` — add `inject_listing_error` Tauri command behind feature flag

**Files to create:**

- Playwright test file for error pane (in the existing E2E test directory)

**Tests:**

- Playwright E2E: trigger error, verify rendering, retry behavior, a11y roles
- Run full `./scripts/check.sh` at the end

### Milestone 4: CLAUDE.md updates + final review

- Update `file_system/volume/CLAUDE.md` with the new `friendly_error.rs` module, its purpose, and the two-layer mapping
  design
- Update `file-explorer/CLAUDE.md` pane section to document `ErrorPane` replacing `PermissionDeniedPane`
- Update `volumes/CLAUDE.md` cloud provider detection section to note that error enrichment uses its own detection in
  `friendly_error.rs` (not `parse_cloud_provider_name`)
- Final writing review of all messages if David has feedback from the M2 visual review

## Key references

- Writing style: `~/Library/CloudStorage/Dropbox/obsidian/agents/David's writing style.md`
- Project style guide: `docs/style-guide.md` (especially the UI error messages section)
- Design principles: `docs/design-principles.md` (radical transparency, platform-native, accessibility)
- Design system: `docs/design-system.md` (color tokens, typography, spacing)
- Existing error display: `FilePane.svelte` lines 659–662 (permission check), 1728–1731 (error display)
- Existing provider detection: `src-tauri/src/volumes/mod.rs` line 570 (`parse_cloud_provider_name` — separate module
  from `file_system/volume/`)
- Existing `VolumeError` enum: `file_system/volume/mod.rs` lines 108–127
- `ListingErrorEvent`: `file_system/listing/streaming.rs` line 68
