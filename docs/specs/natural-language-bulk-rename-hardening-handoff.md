# Natural-language bulk rename hardening handoff

Status captured on 2026-07-21 because the Codex session was nearly out of credits. Continue only in
`/Users/veszelovszki/projects-git/vdavid/cmdr-natural-language-bulk-rename` on branch
`codex/natural-language-bulk-rename`. Do not touch the main working copy. Main was restored clean before this handoff.

PR: [#39](https://github.com/vdavid/cmdr/pull/39)

Last pushed feature commit: `41bc501233c8453fa80e1cd180c2bbc90e00801c`

Codex session: `019f7f8e-e1da-7f23-b5e8-a25f0dde683e`

## Instructions for the next agent

Before touching a folder, manually read `~/.claude/CLAUDE.md`, `~/.claude/rules/*.md`, repo `.claude/*.md`, and every
colocated `CLAUDE.md` from the repository root down to that folder. This harness does not auto-load them. Preserve the
user's edits to `README.md` and `docs/hackathon-submission.md`. Use `pnpm check`, not raw Cargo or Vitest commands. Test
renames only in isolated `_ignored` fixtures.

The worktree contains uncommitted edits from findings 1–4. Do not reset or discard them. Findings 5–7 were deliberately
not started when this handoff was first captured. The scoped follow-up below completed findings 6 and 7; finding 5
remains untouched.

## Scoped follow-up completed after capture

- Replaced the reactive-path plain `Set` in `renameReviewListingChanged` with `SvelteSet`.
- Made the watcher callback await the existing versioned preflight refresh. The overwrite-protection regression now
  observes the authoritative result without sleeps or weakened assertions.
- Added a hard system-prompt rule: when `list_pane_files` returns `truncated: true`, the reply must state it inspected
  only `returned` of `total` files using those exact counts and must not imply full coverage.
- Added a focused prompt test for that rule.
- Removed the obsolete `#[allow(dead_code)]` from `Access::Propose`.
- Linked this handoff from `docs/specs/index.md`.
- Validation after these changes: `pnpm check --fast -q` passed 51 checks with the existing file-length warning;
  `pnpm check svelte-tests --fresh -q` passed the full Svelte lane, including all 6,815 tests.

## Data-safety follow-up completed

- Moved the atomic-exclusive local rename primitive into the shared local POSIX backend and made
  `LocalPosixVolume::rename(..., force = false)` use it. This closes the no-clobber race for attached volumes and local
  cloud roots with non-root IDs, not only the boot volume. Forced renames retain explicit replacement behavior.
- Bulk rename now journals rollback leaves only for `Done` rows.
- Rollback paging filters to `outcome = done`, and `restore_move` defensively skips any non-Done unit that reaches it.
- Added a store regression proving skipped rollback units are excluded.
- Updated volume, backend, write-operation, and operation-log DETAILS docs.
- Validation: the full local Rust lane passed after the changes.

## Current worktree state

Changed or added files at capture time:

- `README.md`
- `docs/hackathon-submission.md`
- `docs/specs/natural-language-bulk-rename-plan.md`
- `apps/desktop/src-tauri/src/agent/tools/propose/DETAILS.md`
- `apps/desktop/src-tauri/src/agent/tools/propose/rename.rs`
- `apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md`
- `apps/desktop/src-tauri/src/file_system/write_operations/rename/bulk.rs`
- `apps/desktop/src-tauri/src/file_system/write_operations/rename/bulk/tests.rs`
- `apps/desktop/src/lib/ask-cmdr/BulkRenameReviewDialog.a11y.test.ts`
- `apps/desktop/src/lib/ask-cmdr/BulkRenameReviewDialog.svelte`
- `apps/desktop/src/lib/ask-cmdr/DETAILS.md`
- `apps/desktop/src/lib/ask-cmdr/ask-cmdr-trigger.svelte.ts`
- `apps/desktop/src/lib/ask-cmdr/ask-cmdr-trigger.test.ts`
- `apps/desktop/src/lib/intl/keys.gen.ts`
- `apps/desktop/src/lib/intl/messages/{de,en,es,fr,hu,nl,pt,sv,vi,zh}/askCmdr.json`
- `apps/desktop/src/lib/ipc/bindings.ts`

The implementation diff for this hardening wave was about 1,337 insertions and 191 deletions before this handoff file.

## Finding 1: atomic no-overwrite and live destination clashes

User finding: the previous check followed by `std::fs::rename` could destroy a destination created in the gap.

Implemented:

- Added `rename_local_exclusive`.
- macOS uses `renamex_np(..., RENAME_EXCL)`.
- Linux uses `renameat2(..., RENAME_NOREPLACE)`.
- The new dependency planner uses the exclusive helper for local final, staged, cycle, and recovery renames.
- Tests cover an empty destination, an occupied destination preserving both files, and a destination appearing after
  review.
- The review dialog subscribes to the existing `directory-diff` event stream.
- It filters changes to proposal source/destination names and reruns authoritative preflight for displayed rows.
- `TargetExists` auto-deselects the row and renders a red `(overwrite!)` badge with an accessible tooltip.
- A row can recover live if the clash disappears, but remains deselected so the user must opt in again.

Reported validation:

- Focused Rust tests passed.
- Formatting, i18n parity, and ICU checks passed.
- Later agents regenerated bindings and translated the new strings.

Review carefully:

- Confirm every local final mutation path uses `rename_local_exclusive`; no check-then-plain-rename final path should
  remain.
- Confirm recovery never clobbers a newly created source/destination. The current planner is intended to use exclusive
  recovery and report failure rather than destroy data.

## Finding 2: dependency-aware execution and cycle warnings

User finding: staging every row doubled rename operations, especially costly on MTP and future object stores.

Implemented by the dependency-planner agent:

- A shared pure planner peels destinations that are not current sources.
- Independent rows execute directly with zero temporary names.
- Acyclic chains execute in reverse dependency/topological order with zero temporary names.
- Each remaining cycle rotates through exactly one temporary name.
- Case-only renames retain one temporary step where required.
- Local and remote execution use the same plan. Remote direct/chain rows use `Volume::rename(..., false)`, avoiding a
  second copy/delete-style rename.
- Cancellation is checked between components. Once a cycle starts, it completes or reverses before cancellation is
  observed.
- Backend preflight emits additive `BulkRenameWarning::Cycle` metadata.
- The dialog shows a yellow, focusable `(cycle)` badge with an accessible tooltip explaining that one temporary name is
  used.
- Copy exists in all supported locales, bindings, DETAILS docs, and the feature spec.

Tests added:

- Independent plan uses no temporary step.
- Chain ordering uses no temporary step.
- Two separate cycles use one temporary step each.
- Case-only behavior uses one temporary step.
- Cancellation/recovery behavior.
- Cycle warning classification and dialog accessibility.
- Existing real chain, cycle, and swap tests reportedly pass.

Reported validation:

- Clippy and rustfmt passed.
- `git diff --check` passed.
- Rust: 4,357 of 4,359 passed, including all new rename tests. Reported unrelated results were a manager panic-lane
  failure and an encoding timeout.
- Svelte: 6,803 passed during that agent's run. The cycle assertion passed; three failures were attributed to concurrent
  source/watcher integration and an unrelated transfer test.

Review carefully:

- Audit the planner rather than trusting the report. Check dependency direction, multiple disconnected components,
  duplicate destinations, case-folded names, partial preflight subsets, cancellation during direct chains, remote
  failure halfway through a component, and recovery when a destination appears concurrently.
- Verify cycle metadata is computed from the exact allowed/preflight subset, not rejected rows.
- Confirm remote implementations actually provide no-overwrite behavior for `force = false`; if a backend cannot, the
  operation must fail safely and document the limitation.

## Finding 3: extension-change warning

User finding: extension changes were visually indistinguishable from ordinary renames.

Implemented:

- Added additive `BulkRenamePreflightRow.warnings: Vec<BulkRenameWarning>`.
- Added `BulkRenameWarning::ExtensionChanged` alongside `Cycle`.
- Extension comparison uses Rust `Path::extension` on the final suffix and compares ASCII-case-insensitively.
- Warns for changed, added, removed, and trailing-dot extensions.
- Does not warn for stem-only changes, extension case-only changes, dotfile-to-dotfile changes, or an unchanged final
  suffix such as one `.tar.gz` name to another `.gz` name.
- Extension changes remain allowed and selected.
- The dialog shows a yellow, focusable `(extension)` badge with a tooltip stating that renaming does not convert file
  contents.
- Generated TypeScript bindings include `warnings` and `BulkRenameWarning`.
- Extension, overwrite, and cycle strings were translated across all nine non-English locales.
- Proposal and Ask Cmdr DETAILS docs were updated.

Reported validation:

- Rust tests passed.
- Binding export passed.
- Eight i18n checks passed.
- `svelte-check` and stylelint passed.
- `git diff --check` passed.
- Full Svelte run: 6,804 passed with two concurrent-integration failures described below.

Review carefully:

- Confirm the intended policy for compound extensions. Current semantics inspect only the final suffix, so
  `archive.tar.gz` to `archive.zip` warns, while `one.tar.gz` to `two.gz` does not.
- Confirm case-only extension changes such as `.PNG` to `.png` should remain warning-free.

## Finding 4: missing sources and live watcher state

User finding: hallucinated or externally deleted sources need a visible blocker and live update.

Implemented before the agent was stopped:

- Existing authoritative local and remote preflight returns `SourceMissing` when metadata/fingerprint lookup fails.
- Missing rows are auto-deselected by generic blocker handling.
- Added a red `(doesn't exist)` badge with an accessible tooltip.
- The same `directory-diff` subscription rechecks matching source/destination names when a source disappears or returns.
- A recovered row remains deselected, matching target-clash recovery behavior.
- Proposal construction was widened only for a nonexistent direct child of the focused local folder, allowing an agent
  hallucination to reach review as `SourceMissing`.
- Existing out-of-scope files, nested/path-escape names, directories, remote hallucinations, and existing files outside
  pane scope remain rejected.

Tests added:

- Rust: a missing local source yields `SourceMissing` and no fingerprint.
- Rust: only nonexistent direct local children may enter review without a pane entry.
- Svelte state: watcher removal blocks/deselects; watcher return rechecks and clears the blocker.
- Dialog/a11y: badge text, tooltip, disabled checkbox, and blocked count.

Reported validation:

- `pnpm check -m --fast -q`: 51 passed with the existing file-length warning.
- Rust: 4,359 of 4,360 passed; the new source test passed. The reported failure was
  `file_system::write_operations::manager::tests::panicking_op_releases_its_lane_without_spawning_next`.
- The full Svelte suite was not rerun after the final source badge implementation.

Known integration problem:

- A trigger watcher test sometimes expects the second mocked `targetExists` preflight result but observes the initial
  `ready` result. This is likely asynchronous mock response ordering/race, not yet reviewed. Fix the test or state flow
  based on actual semantics, not by adding arbitrary sleeps.

Policy question:

- Remote hallucinated source rows are still rejected before review because synchronous proposal construction cannot
  safely prove that an absent remote name is a direct child. Decide whether this is acceptable or whether proposal
  construction should become authoritative/async for remote paths.

## Svelte integration status

The watcher race is resolved. `renameReviewListingChanged` now returns and awaits the versioned preflight refresh, and
the dialog listener explicitly discards that promise. Tests await the returned promise, so they assert the applied
backend result rather than only the start of the IPC call.

The post-fix Svelte lane passed all 6,815 tests. Earlier concurrent-run failures did not recur.

## Remaining finding deliberately not started

### Finding 5: operation-log provenance

Current code at `apps/desktop/src-tauri/src/commands/agent.rs` around line 906 passes only `Initiator::Agent`. The user
requires the operation log to record both facts: the agent proposed the plan and the user approved the selected rows.

Design this as structured provenance, not a misleading replacement with `User` and not a display-only string hack.
Inspect the operation-log schema, serialization, migration requirements, UI presentation, rollback/history consumers,
and existing `Initiator` invariants. Add tests proving both facts survive persistence and appear in the “see what you
did” surface. Update docs.

### Finding 6: mandatory truncation disclosure (completed)

The system prompt now requires exact `returned` of `total` disclosure whenever `truncated` is true and forbids implying
full coverage. A focused prompt test pins all three requirements. A backend rejection of claimed full-folder coverage
was not added; this scoped round changed only the requested prompt contract.

### Finding 7: obsolete dead-code allowance (completed)

The stale allowance and its no-longer-true explanatory comment were removed from `Access::Propose`.

## Integration and review checklist

1. Read all guidance files listed at the top.
2. Inspect `git diff` in full. Three agents edited shared proposal, dialog, trigger, bindings, locale, and docs files.
3. Resolve the watcher test race without sleeps.
4. Run focused proposal/bulk-rename Rust tests and focused Ask Cmdr Svelte tests.
5. Audit the exclusive-rename FFI declarations, flags, errno mapping, and platform cfgs.
6. Audit the dependency planner and failure recovery for data loss before running any manual rename test.
7. If manual testing is needed, use only a newly created directory under `_ignored`.
8. Run binding generation/checks and all i18n checks.
9. Run `pnpm check --fast -q`, then the normal relevant lanes. Do not rerun broad slow/E2E suites unless the user asks.
10. Update this handoff, the implementation spec, DETAILS docs, README submission text, and
    `docs/hackathon-submission.md` to reflect final commits and validation.
11. Commit and push to `codex/natural-language-bulk-rename`, then update PR #39. Do not modify main.

## Safety invariants that must remain true

- The agent can propose but cannot approve or write.
- Approval is a frontend user action over backend-owned proposal rows.
- No final rename may overwrite an unreviewed or concurrently created destination.
- Missing or changed sources remain blocked.
- Rejected rows never enter the execution plan.
- Cancellation and recovery must not clobber new filesystem entries.
- Renames stay within one parent folder; no moves, new folders, or reorganization.
- Image indexing is required only for image-content/OCR reads, not filename/date-only rename proposals.
