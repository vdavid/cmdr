# AI slop review — 2026-02-08

Focus: finding patterns that look AI-generated, over-engineered, or generic across FE, BE, docs, and tooling.

Five parallel review agents covered: Svelte FE, Rust BE, docs, tooling/scripts, and tests.

**Overall verdict: Very clean.** The codebase reads as carefully maintained. Docs are exemplary — zero filler, no passive voice,
strict sentence case. The items below are the only things that stood out.

---

## Findings

### 1. Redundant Rust struct field doc comments

**Impact: Medium (most visible "AI smell") | Size: Quick — batch delete**

`src-tauri/src/file_system/volume/mod.rs` and similar files have doc comments that restate field names:

```rust
pub struct CopyScanResult {
    /// Number of files found.
    pub file_count: usize,
    /// Number of directories found.
    pub dir_count: usize,
    /// Total bytes of all files.
    pub total_bytes: u64,
}

pub struct SpaceInfo {
    /// Total capacity in bytes.
    pub total_bytes: u64,
    /// Available (free) space in bytes.
    pub available_bytes: u64,
    /// Used space in bytes.
    pub used_bytes: u64,
}
```

`/// Number of files found.` on `file_count: usize` is textbook AI output. The field name *is* the doc. Same for
`VolumeError` variants (`/// Path not found` on `NotFound(String)`).

**Rule of thumb:** Keep doc comments only when they add info the name doesn't convey (for example, units, format, edge cases).
`/// Modification time (Unix timestamp in seconds).` on `modified: Option<i64>` is fine.

### 2. Tests that test nothing

**Impact: Medium (false confidence) | Size: Quick — delete two tests**

`src/lib/file-explorer/pane/DualPaneExplorer.test.ts:147-177` has two "infrastructure" tests:

```typescript
it('has infrastructure to persist sort changes via saveColumnSortOrder', async () => {
    // ... calls a mock, asserts the mock was called with what was passed
})

it('has infrastructure to call resortListing command', async () => {
    // ... same pattern — calls mockInvoke, asserts it was called
})
```

These assert nothing about the system. They test that vitest mocks work. Delete them.

### 3. Generic fallback error message

**Impact: Low (one string among ~200 good ones) | Size: Quick — one line**

`src/lib/file-operations/copy/copy-error-messages.ts:147`:

```typescript
return 'An error occurred while copying the file.'
```

Every other error message in that file is specific and helpful. This default fallback sounds like ChatGPT. Replace with
something shorter and more honest, for example `'Copy failed.'` or `'Couldn't copy the file.'`.

### 4. `e.g.` in code comments (~80 instances)

**Impact: Low (style guide violation, not really slop) | Size: Quick — batch find-replace**

The style guide says "Avoid latinisms" but code comments have `e.g.` everywhere:
- ~47 instances in Rust code
- ~33 instances in TypeScript code
- 1 `i.e.` in `wdio.conf.ts`

In code comments `e.g.` is natural and nobody will flag it as AI slop. But it is technically a style guide violation.
Decide whether to care. If yes, it's a quick batch replace (`e.g.` -> `for example`).

### 5. Inconsistent test naming (`should` vs. imperative)

**Impact: Low (cosmetic) | Size: Quick — rename a few tests**

- Smoke tests use imperative: `'app loads successfully'`, `'Tab switches focus between panes'`
- Linux E2E tests use `should`: `'should launch and show the main window'`, `'should display the dual pane interface'`

The `should` style is often associated with AI-generated tests. Unify to imperative.

### 6. Duplicated Go check scripts

**Impact: Low (maintenance burden) | Size: Small — extract helpers**

`desktop-svelte-prettier.go`, `website-prettier.go`, and `license-server-prettier.go` are nearly identical — same for the
three ESLint variants. Extract `runPrettierCheck(dir string)` and `runESLintCheck(dir string)` into `common.go`.

## Task list

- [x] Strip redundant struct field doc comments in Rust (volume/mod.rs and similar) `[medium impact, quick]` - make sure to not just blindly remove all docs, keep the info that's not tautological.
- [ ] Delete two meaningless "infrastructure" tests in DualPaneExplorer.test.ts:147-177 `[medium impact, quick]`
- [ ] Replace generic error fallback in copy-error-messages.ts:147 `[low impact, quick]` - make it in line with the style guide: be helpful, transparent, etc.
- [ ] Batch-replace `e.g.` in code comments if desired `[low impact, quick]` - thoughtful replacing pls.
- [ ] Unify E2E test naming to imperative style (drop `should`) `[low impact, quick]`
- [ ] Extract shared prettier/eslint helpers in Go check scripts `[low impact, small]`
