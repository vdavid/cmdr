## Agent 1's report

Prompt was: I try to gradually review my codebase (FE, BE, docs, tooling) and find weak spots. Actually, I haven't done this in a while so I'm looking more for like places where the house is burning. Most of the codebase is vibe coded with my overview. Do a thorough review, use commands/subagents/whatever to go through it in some depth and advise for things that are very much off.

1. License server: Math.random() for license codes

apps/license-server/src/license.ts:25

const randomIndex = Math.floor(Math.random() * chars.length)

Math.random() is not cryptographically secure. Your license codes (CMDR-XXXX-XXXX-XXXX) have ~43 bits of entropy from a predictable PRNG. On Cloudflare Workers, crypto.getRandomValues() is available and should be
used instead. Someone could potentially predict/brute-force license codes.

Fix: Replace with crypto.getRandomValues(new Uint8Array(12)) and map to your character set.

  ---
2. License server: fake "constant-time" HMAC comparison

apps/license-server/src/paddle.ts:35-36

// Constant-time comparison
return signature === expectedSignature

The comment says "constant-time" but === is NOT constant-time. This is a textbook timing attack vector on your Paddle webhook signature verification. An attacker can probe byte-by-byte to forge valid webhook
signatures.

Fix: Use crypto.subtle.timingSafeEqual (available on CF Workers) or a manual constant-time comparison.

  ---
3. CSP completely disabled in production

apps/desktop/src-tauri/tauri.conf.json:32

"security": {
"csp": null
}

null CSP means no Content Security Policy at all in your production Tauri app. If there's any XSS vector (and with file operations + network shares, there could be), an attacker gets unrestricted script execution
inside the webview with access to all Tauri IPC commands. This is the keys to the kingdom for a file manager.

Fix: Define a restrictive CSP. At minimum: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline' (adjust for your actual needs).

  ---
4. Condvar wait with no timeout = thread hangs forever

apps/desktop/src-tauri/src/file_system/write_operations/helpers.rs:359-371

let guard = state.conflict_mutex.lock().unwrap();
let _guard = state
.conflict_condvar
.wait_while(guard, |_| {
let has_resolution = state.pending_resolution.read()...;
let is_cancelled = state.cancelled.load(Ordering::Relaxed);
!has_resolution && !is_cancelled
})
.unwrap();

If the user closes the conflict dialog without responding (or the frontend crashes/disconnects), this thread blocks forever. No timeout. The copy operation hangs silently, holding a mutex, potentially leaking
resources.

Fix: Use wait_timeout_while with a reasonable timeout (say 5 minutes), then auto-cancel.

  ---
5. 67 .lock().unwrap() calls across production code

Confirmed: 67 occurrences in 15 files (some in tests, but many in production paths like lib.rs, network/mod.rs, file_viewer/session.rs, menu.rs, commands/settings.rs).

If ANY thread panics while holding a mutex, all subsequent .lock().unwrap() on that mutex will cascade-panic, crashing the entire app. This is especially risky in the network/bonjour code where you're calling
into OS APIs that could fail in unexpected ways.

Fix: At minimum, replace with .lock().expect("context message") so crashes are diagnosable. Better: use lock().unwrap_or_else(|e| e.into_inner()) where recovering from a poisoned mutex is safe (which it usually
is for this kind of state).

  ---
6. SMB username not URL-encoded

apps/desktop/src-tauri/src/network/smb_smbutil.rs:152

format!("//{}:{}@{}", username, encoded_password, host)

Password is URL-encoded, username is not. A username containing @, :, or / (all valid in some auth systems) will break the URL format. It's passed as a command argument (not shell-injected), so this isn't RCE,
but it will cause confusing failures for users with special-character usernames.

Fix: Also encode the username: urlencoding::encode(username).

  ---
7. GitHub Actions: unpinned action tags

.github/workflows/release.yml:21

uses: jdx/mise-action@v2

Using a mutable @v2 tag rather than a pinned SHA. If this action's repo gets compromised, attackers get code execution in your release pipeline (which has contents: write permission and likely access to signing
keys). This is a supply chain risk.

Fix: Pin to a specific commit SHA. Same for any other @v2-style references in your workflows.

  ---
8. Zero Svelte component tests

52 .svelte files, 0 .svelte.test.ts files.

All Svelte tests are in apps/desktop/test/ and test utility functions, not components. The entire file explorer UI (FilePane, DualPaneExplorer, dialogs, etc.) has no component-level tests. The smoke E2E tests
cover only basic rendering and Tab switching. No E2E tests for copy, move, delete, rename, or any file operation flow.

This means the most user-facing, most complex part of the codebase (the dual-pane explorer with keyboard-driven interactions) is essentially untested. Regressions in component behavior, event handling, and state
management won't be caught.

  ---
Summary by priority
┌─────┬───────────────────────────────────┬──────────────────────────────┬────────────────┐
│  #  │               Issue               │             Risk             │     Effort     │
├─────┼───────────────────────────────────┼──────────────────────────────┼────────────────┤
│ 1   │ Math.random() in license codes    │ Predictable codes            │ Quick fix      │
├─────┼───────────────────────────────────┼──────────────────────────────┼────────────────┤
│ 2   │ Non-constant-time HMAC comparison │ Webhook forgery              │ Quick fix      │
├─────┼───────────────────────────────────┼──────────────────────────────┼────────────────┤
│ 3   │ CSP disabled in production        │ XSS → full app control       │ Medium         │
├─────┼───────────────────────────────────┼──────────────────────────────┼────────────────┤
│ 4   │ Condvar infinite wait             │ Thread hangs, resource leaks │ Quick fix      │
├─────┼───────────────────────────────────┼──────────────────────────────┼────────────────┤
│ 5   │ 67 .lock().unwrap() calls         │ Cascade crash                │ Medium (batch) │
├─────┼───────────────────────────────────┼──────────────────────────────┼────────────────┤
│ 6   │ SMB username not encoded          │ Broken auth for some users   │ Quick fix      │
├─────┼───────────────────────────────────┼──────────────────────────────┼────────────────┤
│ 7   │ Unpinned GH Actions               │ Supply chain attack          │ Quick fix      │
├─────┼───────────────────────────────────┼──────────────────────────────┼────────────────┤
│ 8   │ Zero component tests              │ Silent regressions           │ Ongoing        │
└─────┴───────────────────────────────────┴──────────────────────────────┴────────────────┘
Items 1, 2, and 4 are genuinely quick fixes (under 30 minutes total). Item 3 needs a bit of thought about which CSP rules you need. Items 5 and 7 are batch-replaceable. Item 8 is a longer-term investment.

Frontend extras worth mentioning:
- $effect listener cleanup race (FilePane.svelte:1060-1093): The listen() calls in $effect blocks use .then() chains instead of await, so the cleanup function can return before the listener is actually         
  registered. If the effect re-fires quickly (navigating directories fast), you accumulate zombie listeners.
- Copy operation ID capture race (CopyProgressDialog.svelte:140): When operationId is null, the first arriving event sets it. If a stale event from a previous copy arrives first, the dialog tracks the wrong    
  operation.
- Selection cleared before directory load (FilePane.svelte:~500): If loading a new directory fails, the user's selection is already gone.

The top things I'd fix today if I were you:

1. License server quick fixes (~15 min): Math.random() → crypto.getRandomValues(), and === → proper constant-time comparison
2. Condvar timeout (~15 min): wait_while → wait_timeout_while
3. CSP (~30 min): Define a real policy in tauri.conf.json
4. Pin GH Actions (~10 min): SHA-pin mise-action and tauri-action

## Agent 2's report

I try to gradually review my codebase (FE, BE, docs, tooling) and find signs of AI slop. I use AI extensively but don't want people to say that my code is generated/dumb/sloppy. Do a thorough review, use     
commands/subagents/whatever to go through it in some depth and advise for things that are very much off.

1. Tests that test nothing -- licensing.test.ts

The entire 329-line file mocks every function, calls the mock, then asserts the mock returned what it was told to return. Zero actual logic tested.

// Line 41-47 — this pattern repeats ~20 times
it('handles personal license status', async () => {
const mockStatus: LicenseStatus = { type: 'personal', showCommercialReminder: false }
vi.mocked(getLicenseStatus).mockResolvedValue(mockStatus)
const status = await getLicenseStatus()
expect(status.type).toBe('personal')  // tests vitest, not your app
})

2. Type-shape tests -- streaming-loading.test.ts:204-302

Seven tests that construct a TypeScript object and assert its fields equal the values just set. TypeScript's compiler already guarantees this.

it('StreamingListingStartResult has correct shape', () => {
const result: StreamingListingStartResult = { listingId: 'test-123', status: 'loading' }
expect(result.listingId).toBe('test-123')  // you literally just typed this
})

3. Inline-reimplementation tests -- streaming-loading.test.ts:308-523 and integration.test.ts:944-1031

Tests that define local functions mimicking app logic, then test the local copy. If the real code changes, these still pass. Worst example:

// integration.test.ts:1008 — tests that `true` is `true`
it('F1 opens left pane volume chooser', () => {
let leftVolumeChooserOpened = false
function handleF1() { leftVolumeChooserOpened = true }
handleF1()
expect(leftVolumeChooserOpened).toBe(true)
})

4. 30x identical .map_err() lambda -- mcp/executor.rs

The pattern .map_err(|e| ToolError::internal(e.to_string())) appears ~30 times in this single file. Screams "I asked AI to add error handling and it copy-pasted."

Moderate severity (noticeable on close read)

5. 5x copy-pasted operationId filtering -- CopyProgressDialog.svelte

This exact block is duplicated in handleProgress, handleComplete, handleError, handleCancelled, handleConflict:
if (operationId === null) {
operationId = event.operationId
} else if (event.operationId !== operationId) { return }
Should be a one-liner helper.

6. No-op polling interval -- FilePane.svelte:1179-1181

An setInterval with an empty callback running every 2 seconds. Creates a timer, adds cleanup code, achieves nothing.

7. Duplicated dialog drag logic -- CopyDialog.svelte + CopyProgressDialog.svelte

~30 identical lines of handleTitleMouseDown + dialogPosition state + identical CSS. Should be a shared draggable dialog utility.

8. Duplicated MTP error parsing -- MtpConnectionView.svelte + mtp-store.svelte.ts

Nearly identical "parse unknown error from Tauri" try/catch chains. Should be a shared utility.

9. Identical ternary branches -- DualPaneExplorer.svelte:1012-1014

const path = currentPath === '~' ? `${currentPath}/${filename}` : `${currentPath}/${filename}`
Both branches are the same. Classic unreviewed AI output.

10. path: String::new() in error conversions -- volume_copy.rs:535-552

Three match arms in map_volume_error() set path: String::new(), meaning error messages lose their file context. The caller has the path but doesn't pass it.

11. Verbose JSDoc repeating function names -- multiple files

~12 functions in write-operations.ts, ~20 exports in FilePane.svelte, multiple in DualPaneExplorer.svelte. Example: /** Get the focused pane identifier. */ for getFocusedPane(). The "Used by MCP" notes are
useful; the name restatements are noise.

12. Verbose # Arguments doc blocks on one-line Rust wrappers -- commands/file_system.rs

~15 Tauri command wrappers that are single-line pass-throughs to another function, each with multi-line # Arguments / # Returns doc blocks.

Low severity (minor cleanup when touching these files)

13. Duplicated CSS patterns across components

- .size-bytes/.size-kb/.size-mb/etc. color classes duplicated in 3 files (FullList, CopyProgressDialog, SelectionInfo)
- .close-button styles duplicated in 4 files
- .modal-overlay + button styles duplicated in 3+ dialog files
- Monospace font stack hard-coded in 3 files instead of a CSS variable
- Hardcoded rgba(10, 80, 208, 0.1) in 2 files instead of a design token

14. e.g. / etc. in docs -- style guide violation

~80+ instances of e.g. and etc. across docs, mostly in specs and feature docs. The style guide says to avoid latinisms. The active feature docs and guides are the priority; archived specs can wait.

15. Title Case headings in a few docs

mtp-library-info.md: "Error Handling", "Architecture Recommendation", "Key Points Summary". A couple others in e2e-testing-guide.md and linux-and-windows versions docs.

16. Dead code

- formatLicenseKey() in license-server/src/license.ts -- identity function, comment says "no longer used"
- Stale monorepo fallback path in scripts/check/utils.go -- detects a layout that no longer exists
- Commented-out CSS in FullDiskAccessPrompt.svelte (/*background: ...*/, /*font-family: ...*/)
- Vite config with stock Tauri scaffold comments

17. benchmarking-file-loading.md opens with "A comprehensive guide to..."

Classic AI opener. The style guide says to be friendly and concise -- drop "comprehensive" and just get into it.



1. Delete or rewrite bad tests (#1-3) -- these provide false confidence and are the most visible slop
2. Extract the map_err helper (#4) -- quick win, very visible repetition
3. Extract shared utilities (#5, 7, 8) -- operationId filter, dialog drag, error parsing
4. Fix dead code (#6, 9, 16) -- no-op interval, identical ternary, dead formatLicenseKey
5. Trim verbose JSDoc (#11, 12) -- bulk-remove restatement comments, keep the useful "why" notes
6. CSS dedup (#13) -- next time you touch dialog or list components
7. Docs cleanup (#14, 15, 17) -- batch find-replace e.g. and Title Case headings

Additional Rust findings worth noting:

- scan_preview_recursive vs scan_path_recursive -- two near-identical recursive directory walkers in scan.rs that differ only in event types. Should share a common walker.
- response_to_app_status vs cached_to_app_status -- identical match blocks in app_status.rs that should be one function.
- Hidden-file filter !e.name.starts_with('.') repeated 12 times in operations.rs without ever extracting a helper.
- Three identical cancellation poll-loop wrappers (spawn thread + channel + poll) in copy.rs and scan.rs -- should be a generic run_cancellable helper.
- Tautological Rust test in app_status.rs:613-663 -- constructs enum variants and matches them against themselves. Same pattern as the TypeScript tests.
- "Will be used in Phase N" dead code allows in mtp/connection/mod.rs -- classic AI forward-planning stubs with no tracking of whether those phases arrived.

These slot into the existing severity tiers -- the scan walker duplication and 12x hidden-file filter are probably the highest-impact Rust fixes since they're real maintenance burden, not just cosmetic.

## Task list

### Critical — security vulnerabilities

- [x] Replace Math.random() with crypto.getRandomValues() in license code generation `[quick fix]`
- [x] Replace === with constant-time comparison for Paddle webhook HMAC verification `[quick fix]`
- [x] Pin GitHub Actions to commit SHAs instead of mutable tags `[quick fix]`
- [x] Define a real CSP in tauri.conf.json `[medium]`

### High — reliability bugs, tests providing false confidence

- [x] Add timeout to condvar wait in conflict resolution (wait_while → wait_timeout_while) `[quick fix]`
- [x] Fix $effect listener cleanup race in FilePane.svelte (.then() → proper async cleanup) `[medium]`
- [x] Fix copy operation ID capture race in CopyProgressDialog.svelte `[quick fix]`
- [x] Delete or rewrite licensing.test.ts (329 lines of mock-the-mock tests) `[medium]`
- [x] Delete type-shape tests in streaming-loading.test.ts:204-302 `[quick fix]`
- [x] Delete or rewrite inline-reimplementation tests in streaming-loading.test.ts + integration.test.ts `[medium]`
- [x] Delete tautological Rust test in app_status.rs:613-663 `[quick fix]`

### Medium — bugs, visible duplication

- [x] Replace 67 .lock().unwrap() calls with .unwrap_or_else(|e| e.into_inner()) or .expect() `[medium — batch]`
- [x] URL-encode SMB username in smb_smbutil.rs `[quick fix]`
- [x] Extract .map_err(|e| ToolError::internal(e.to_string())) helper in mcp/executor.rs (~30 occurrences) `[quick fix]`
- [x] Extract operationId filtering helper in CopyProgressDialog.svelte (5 duplications) `[quick fix]` *(already done)*
- [x] Remove no-op setInterval in FilePane.svelte:1179-1181 `[quick fix]`
- [x] Fix identical ternary branches in DualPaneExplorer.svelte:1012-1014 `[quick fix]`
- [x] Pass path context to map_volume_error() instead of String::new() in volume_copy.rs `[quick fix]`
- [ ] Fix selection cleared before directory load in FilePane.svelte `[not quick — needs architectural change]`
- [ ] Unify scan_preview_recursive and scan_path_recursive into a shared walker `[medium]`
- [x] Extract hidden-file filter helper (repeated 12 times in operations.rs) `[quick fix]`
- [ ] Extract generic run_cancellable helper from 3 identical poll-loop wrappers in copy.rs/scan.rs `[medium]`
- [x] Unify response_to_app_status and cached_to_app_status into one function `[quick fix]`

### Low — cleanup, style, docs

- [ ] Extract shared draggable dialog utility from CopyDialog + CopyProgressDialog (~30 duplicate lines) `[medium]`
- [ ] Extract shared MTP error parsing utility from MtpConnectionView + mtp-store `[quick fix]`
- [ ] Remove verbose JSDoc that restates function names (~32 functions across TS/Svelte files) `[medium — batch]`
- [ ] Remove verbose # Arguments doc blocks on one-line Rust wrappers in commands/file_system.rs `[medium — batch]`
- [ ] Deduplicate CSS patterns: size-color classes, .close-button, .modal-overlay, monospace font stack, hardcoded colors `[medium]`
- [ ] Remove dead code: formatLicenseKey(), stale Go monorepo path, commented-out CSS, scaffold comments `[quick fix]`
- [ ] Remove "Phase N" dead code stubs in mtp/connection/mod.rs `[quick fix]`
- [x] Replace ~80 instances of "e.g." and "etc." in active docs `[quick fix — batch]`
- [x] Fix Title Case headings in mtp-library-info.md, e2e-testing-guide.md, and others `[quick fix]`
- [x] Rewrite "A comprehensive guide to..." opener in benchmarking-file-loading.md `[quick fix]`

### Ongoing

- [ ] Add Svelte component tests (52 .svelte files, zero component-level tests today)

