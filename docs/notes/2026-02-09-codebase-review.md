# Codebase review — 2026-02-09

Thorough review of the full codebase (FE, BE, license server, CSS, docs, tests) using parallel automated
analysis + manual verification. Focused on "where the house is burning" — not style nits.

## Findings

### 1. License server: no webhook idempotency (critical)

**`apps/license-server/src/index.ts:156-234`**

If Paddle retries a webhook (network timeout, 5xx — Paddle retries up to three times), the same transaction gets
processed again: new license codes generated, new email sent. Customer gets duplicate codes, KV fills with orphan
licenses.

**Fix:** Check KV for `transaction:{transactionId}` before processing. Store after success with a TTL.

### 2. License server: whole webhook handler is unprotected (critical)

**`apps/license-server/src/index.ts:170`** — `JSON.parse(body)` has no try-catch. If Paddle sends malformed JSON,
unhandled exception.

**`apps/license-server/src/index.ts:218-230`** — `generateAndSendLicenses` (KV writes + email sending) has no
try-catch. If email sending fails partway through, licenses are generated in KV but never delivered. No retry, no
alert. Customer paid but got nothing.

**Fix:** Wrap the whole handler in try-catch. For the license generation loop, either make it all-or-nothing or add
retry logic for the email.

### 3. License server: timing attack on admin auth + audit for more (high)

**`apps/license-server/src/index.ts:322`**

```typescript
const isAuthorized = validSecrets.some((secret) => authHeader === `Bearer ${secret}`)
```

Uses JS `===` which short-circuits on first mismatch. `constantTimeEqual()` already exists in `paddle.ts` for webhook
verification but isn't used here. An attacker with network access could gradually reveal the bearer token byte by byte.

**Fix:** Use `constantTimeEqual()` for admin auth. Also audit the rest of the license server for any other
string comparisons against secrets or tokens that should be constant-time.

### 4. `--color-allow` has no dark mode definition (high)

**`apps/desktop/src/app.css:44`** defines `--color-allow: #2e7d32` (dark green) in the light theme, but the dark mode
block (lines 78–118) has no override. Every other semantic color (`--color-error`, `--color-warning`) has a dark mode
variant.

Dark green (#2e7d32) on dark backgrounds (#1e1e1e) = nearly invisible. Affects:
- Copy dialog success indicators
- Permission denied UI
- Full disk access prompt
- MCP server status

**Fix:** Add `--color-allow` (lighter green) to the dark mode block.

### 5. AppleScript injection in `get_info` + audit for similar escaping gaps (medium-high)

**`apps/desktop/src-tauri/src/commands/ui.rs:138`**

```rust
let escaped_path = path.replace("\\", "\\\\").replace("\"", "\\\"");
```

Only escapes `\` and `"`. A filename containing `") & (do shell script "malicious")` could theoretically break out.
In practice, the path comes from directory listings (so the file must actually exist on disk), which limits the attack
surface — but it's still sloppy for something that shells out to `osascript`.

**Fix:** Use `NSWorkspace` via objc bindings or Finder's Apple Events API directly rather than string-interpolated
AppleScript. Also audit the rest of the Rust codebase for other places where user-controlled strings are interpolated
into shell commands or scripts without proper escaping.

### 6. Viewer: hardcoded highlight colors bypass theme (medium)

**`apps/desktop/src/routes/viewer/+page.svelte:849-868`**

Search highlight `<mark>` uses hardcoded hex colors (`#fff3a8`, `#ff9632`, `#665d20`, `#cc6600`) with a
`@media (prefers-color-scheme: dark)` block instead of CSS variables. The rest of the app uses `--color-highlight`
(defined in `app.css:55`). This means highlight colors won't respond to any future theme system.

**Fix:** Use `var(--color-highlight)` and define a `--color-highlight-active` variable.

### 7. License server: user input in HTML emails not escaped (medium)

**`apps/license-server/src/email.ts`**

`customerName` and `organizationName` are interpolated directly into HTML email templates without escaping. If Paddle's
`custom_data` contains HTML, it ends up in the email raw. Resend might sanitize, but relying on that is fragile.

**Fix:** HTML-escape user inputs before interpolation.

### 8. License server: no input validation on admin endpoint + audit for more (medium)

**`apps/license-server/src/index.ts:327-331`**

The `/admin/generate` endpoint accepts arbitrary `email` (no format validation), `type` (no runtime enum check, just
TypeScript hint), and `organizationName` (no length limit). A 100MB org name string would bloat KV storage.

**Fix:** Add runtime validation for all inputs on this endpoint. Also audit all other license server endpoints
(`/activate`, `/validate`, `/webhook/paddle`) for missing input validation — check string lengths, format constraints,
and type coercion issues.

### 9. 20+ completed specs still in `docs/specs/` (low)

Every completed feature spec is still in `docs/specs/` alongside active ones. Viewer, settings, shortcut, MTP specs are
all marked `[x]` complete. Per AGENTS.md, these are supposed to be "temporary spec docs."

**Fix:** Delete completed specs. Git history has them if we ever need to look back.

### 10. PII in production logs + audit for more (low)

**`apps/license-server/src/index.ts:201,232`**

```typescript
console.log('Customer email:', customer.email, 'business:', customer.businessName)
```

Customer emails logged in production Cloudflare Worker logs. Depending on log retention settings and GDPR posture,
this could be a compliance issue.

**Fix:** Remove or redact PII from production logs. Also audit the rest of the license server (and Rust backend
`log::info!`/`log::debug!` calls) for any other places where user data, file paths, or credentials might end up in
logs.

### 11. `delete.rs` has zero tests (high)

**`apps/desktop/src-tauri/src/file_system/write_operations/delete.rs` (157 lines)**

Delete is not an active user-facing feature yet — the code is prep for its implementation. But the backend logic is
already written and should be solid before we build the UI on top of it. Cancellation mid-delete, permission denied on
nested items, and partial deletion states are all untested.

**Fix:** Add Rust tests for `delete.rs` covering: successful delete, cancellation, permission errors, nested directory
deletion, and partial failure scenarios. This way the backend is solid when we wire up the feature.

### 12. Cross-filesystem move staging pattern untested (medium)

**`apps/desktop/src-tauri/src/file_system/write_operations/move_op.rs` (415 lines)**

If the system crashes between "copy to staging dir" and "rename staging to final", you get orphaned files in both
locations. The staging pattern, atomic rename failure handling, and rollback-on-failure are all untested.

**Fix:** Add tests for the cross-filesystem move path, especially crash/failure scenarios during staging.

## What looked fine

- **Rust backend overall**: Solid error handling, correct file watcher debouncing, proper MTP connection cleanup. The
  poisoned-mutex pattern (`unwrap_or_else(|e| e.into_inner())`) is ugly but technically correct.
- **Svelte frontend**: Timer cleanup in the viewer is well done (`onDestroy` at line 621 cleans up intervals and
  debounce timers). The `void` fire-and-forget pattern is used correctly.
- **License crypto**: Ed25519 implementation is solid. Unambiguous character alphabet, rejection sampling for short
  codes, constant-time comparison for webhook verification.
- **Test coverage for viewer**: 64 Rust tests for the viewer backend alone.
- **AGENTS.md**: Accurate and well-maintained. File structure matches reality.

## Task list

### Immediate (critical, affects real money)

- [ ] Add webhook idempotency — check KV for existing transaction before processing (#1) `[S]`
- [ ] Wrap webhook handler in try-catch, handle email/KV failures gracefully (#2) `[S]`

### Urgent (high severity)

- [ ] Fix admin auth timing attack + audit for other constant-time comparison gaps (#3) `[S]`
- [ ] Add `--color-allow` to dark mode CSS block (#4) `[XS]`
- [ ] Add Rust tests for `delete.rs` — success, cancellation, permission errors, partial failure (#11) `[M]`

### Soon (medium severity)

- [ ] Fix AppleScript injection + audit codebase for other shell/script interpolation gaps (#5) `[M]`
- [ ] Replace hardcoded viewer highlight colors with CSS variables (#6) `[XS]`
- [ ] HTML-escape user inputs in license email templates (#7) `[XS]`
- [ ] Add input validation to admin endpoint + audit other endpoints (#8) `[S]`
- [ ] Add tests for cross-filesystem move staging pattern (#12) `[M]`

### When convenient (low severity)

- [ ] Delete completed specs from `docs/specs/` (#9) `[XS]`
- [ ] Remove/redact PII from production logs + audit for more (#10) `[S]`
