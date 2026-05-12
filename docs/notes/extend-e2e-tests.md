# Extend E2E tests — coverage-driven walk

Status: in progress.

## Plan

| #   | Spec                       | Status  | Tests before | Tests added | Notes |
| --- | -------------------------- | ------- | ------------ | ----------- | ----- |
| 1   | settings.spec.ts           | done    | 5            | 3           | sidebar filter, no-match, arrow-key from search input |
| 2   | git-portal.spec.ts         | done    | 2            | 2           | tags/v1.0.0, commits/ short-SHA listing |
| 3   | viewer.spec.ts             | done    | 10           | 1           | no-matches search state (W-key wrap test dropped, see notes) |
| 4   | indexing.spec.ts           | done    | 3            | 0           | already byte-exact end-to-end; no gap warrants e2e |
| 5   | network-toggle.spec.ts     | done    | 4            | 0           | existing 4 cover the user-visible UX; deeper coverage is unit-level |
| 6   | error-pane.spec.ts         | done    | 3            | 1           | folder-path display + collapsed-details disclosure |
| 7   | file-operations.spec.ts    | done    | 8            | 0           | thorough already; rename-conflict gap noted below |
| 8   | conflict-move.spec.ts      | done    | 3            | 0           | exhaustive merge / skip / rollback coverage |
| 9   | conflict-copy.spec.ts      | done    | 7            | 0           | policy matrix already saturated |
| 10  | conflict-edge-cases.spec.ts| done    | 7            | 0           | rollback + symlinks + type mismatches all covered |
| 11  | app.spec.ts                | done    | 14           | 2           | F7-Cancel button and F8 delete confirm dialog |
| 12  | accessibility.spec.ts      | done    | 20           | 0           | already covers main flows in light + dark |
| 13  | file-watching.spec.ts      | done    | 11           | 0           | CRUD + batch + threshold + dedup all covered |
| 14  | mtp.spec.ts                | done    | 21           | 0           | 21 tests across browse/copy/move/delete/rename/read-only |
| 15  | mtp-conflicts.spec.ts      | done    | 5            | 0           | move-conflict matrix saturated |
| 16  | smb.spec.ts                | skip    | —            | —           | Out of scope per brief |

## Per-spec analysis

### settings.spec.ts

**Source surface**: `src/routes/settings/+page.svelte`, `src/lib/settings/components/SettingsSidebar.svelte`, `settings-search.ts`, `settings-registry.ts`.

**Behaviors covered (before)**:
- Renders, sidebar shows sections, expected section names present, search input accepts text, clicking a sidebar item selects it.

**Gaps identified**:
- Search debounce + sidebar filtering (search actually narrows the visible list).
- Empty-result branch (`zzzyyyxxxnomatch`-style query): sidebar collapses to zero items, clear (×) recovers.
- Clear-search button (`.search-clear`) round-trip.
- Arrow Up/Down in the search box drives section selection (the search box has its own `handleSearchKeydown` separate from the section-tree listbox).
- Escape closes the settings window (out of scope — the spec runs many tests that need the window open).
- `?section=...` URL deep-link (out of scope — requires a window reload).
- `navigate-to-section` Tauri event (covered indirectly by the volume picker test; testing it from within the Settings window's own context is non-load-bearing).
- Last section persistence (`saveLastSettingsSection`) — also needs a reload.

**Tests added** (3):
1. `search narrows the visible sidebar sections and clearing restores them` — drives the debounced filter with `accent` (matches one Appearance row), then clicks the `×` and asserts the full list is back.
2. `search shows an empty sidebar for queries with no matches` — covers the no-match branch and confirms the clear button is still reachable; cleans up search state for the next test.
3. `Arrow Down in the search box moves section selection forward` — covers the dual-keydown path in `SettingsSidebar.handleSearchKeydown` (Arrow keys in input forward to `navigateSections`); clears any leftover search up front so a `.selected` row is present.

**Skipped (with reason)**: Escape-closes-window, URL deep-link, last-section persistence — all need a window reload that the shared test suite isn't set up to do cleanly.

### git-portal.spec.ts

**Source surface**: `src-tauri/src/file_system/git/{virtual_listing,path,tree}.rs`. Frontend pane orchestration is generic; the portal lives entirely in the volume hooks.

**Behaviors covered (before)**: 2 active tests (portal root entries; branches/main tree) + 2 skipped (cross-volume copy; portal toggle), both documented.

**Gaps identified**:
- `tags/<tag>` navigation (exercises `resolve_ref_commit`, including annotated-tag peel and the dot-bearing ref parse in `classify`).
- `commits/` listing (exercises `list_commits` end-to-end via the volume hook; M3-era code path).
- Friendly error rendering inside the portal (would need an injected gix error; `error-pane.spec.ts` already covers FriendlyError for filesystem errors and a Rust test covers it for git).

**Tests added** (2):
1. `navigates tags/v1.0.0 and sees the tree at the tagged commit` — covers the tag-resolving branch and dot-in-ref parser path.
2. `navigates commits/ and shows the single HEAD commit by short SHA` — covers `list_commits` integration via the listing pipeline; regex-checks for a 7+ hex name to avoid pinning to a specific SHA across git versions.

**Skipped (with reason)**: Friendly git error rendering — already covered by Rust unit tests + the broader friendly-error path is exercised by `error-pane.spec.ts`.

### viewer.spec.ts

**Source surface**: `src/routes/viewer/+page.svelte` plus the composables `viewer-search.svelte.ts`, `viewer-scroll.svelte.ts`, `viewer-line-heights.svelte.ts`. Backend: `src-tauri/src/file_viewer/`.

**Behaviors covered (before)**:
- Render container, line elements, file name in status bar, line count, file size, backend badge, Ctrl+F opens search, search finds matches, Escape closes search, missing-path error.

**Gaps identified**:
- No-match search state (UI says "No matches"). This exercises the "done" status branch of `searchStatus` and confirms `aria-live` content.
- W toggles word wrap (cross-state setting + CSS class flip).
- Enter advances to next match (already covered indirectly because `findMatches` test pulls a match count, but no test confirms navigation).
- F3 from file list opens viewer (cross-component; opens a NEW Tauri window, outside the test's single-window scope — defer).
- Line heights variant testing (FullLoad pretext path) — deep internal, deferred.

**Tests added** (1):
1. `shows "No matches" status for a query with no hits` — fills with `Z * 40` (the fixture is `A` × 1024 so cannot match), polls the `.match-count` aria-live region for "No matches". Resets the query in cleanup.

**Skipped (with reason)**: F3-opens-viewer (cross-window flow), line-heights internals (tier 3 candidates), W toggles word wrap (the synthetic keydown reaches `<svelte:window on:keydown>` but doesn't flip the wrap class reliably; investigating focus / `viewerSetWordWrap` IPC side effects would consume disproportionate time for a single-key path — deferred with a TODO).

### indexing.spec.ts

**Source surface**: `src-tauri/src/file_system/index/` (renamed `indexing` in the codebase). Frontend reads via `get_dir_stats`.

**Behaviors covered (before)**: 3 thorough tests: initial dir size from index, exact-byte increase on file creation, exact-byte decrease on file deletion. UI-side numeric size in Full view also checked.

**Gaps identified**:
- All key flows are already covered. The Scanning... → numeric transition is implicitly covered. Edge cases (non-existent path → null, very large directories) would be unit-level.

**Tests added**: 0. **Reason**: The existing suite hits the indexing pipeline end-to-end with byte-exact assertions for both create and delete. Adding more would either duplicate or descend into Rust-side unit territory.

### network-toggle.spec.ts

**Source surface**: `src/lib/volumes/` (frontend volume picker), `src-tauri/src/file_system/volume/network/` (backend mDNS).

**Behaviors covered (before)**: Default label, toggle-off label, toggle-back label, click-disabled-leaves-volume-unchanged.

**Gaps identified**:
- Settings deep-link to Network section when clicking the disabled entry — `settings-window.ts` emits a `navigate-to-section` event. Already exists implicitly in code, and the test author explicitly notes inspecting the settings window is awkward via `evaluate()`.
- mDNS-actually-stops behavior — unobservable from the UI side.

**Tests added**: 0. **Reason**: Existing tests cover the user-observable UX cleanly. Backend mDNS-stop is unit-level. The deep-link assertion would require spawning the settings window from the test, which is structurally fragile.

### error-pane.spec.ts

**Source surface**: `src/lib/file-explorer/pane/ErrorPane.svelte` + `src-tauri/src/file_system/listing/friendly_error.rs` (error classification).

**Behaviors covered (before)**: ETIMEDOUT (transient with retry), retry-clears-error-state, EACCES (NeedsAction without retry), accessibility (role/heading).

**Gaps identified**:
- Folder path display (user must see WHICH directory failed).
- `<details>` technical-details disclosure default-collapsed + click-to-expand.
- Retry info text rendering after multiple clicks (deep UX; gated by hitting retry repeatedly within seconds).
- `x-apple.systempreferences:` link handling — unit-testable; production-impactful but doesn't load-bear here.

**Tests added** (1):
1. `shows the offending folder path and a collapsed technical details disclosure` — injects ETIMEDOUT, asserts `.folder-path` ends with `/left/sub-dir`, then verifies `<details>` starts without the `open` attribute and gains it after clicking the summary.

**Skipped (with reason)**: Retry-info-after-multi-click — feels like UX polish coverage; deferred unless we see regressions.

### file-operations.spec.ts

**Source surface**: `src-tauri/src/file_system/write_operations/{copy,move,rename,mkdir}.rs`. Frontend: `src/lib/file-operations/**`, `src/lib/file-explorer/views/**`.

**Behaviors covered (before)** (8): F5 copy, F6 move, F2 rename, F7 mkdir, view mode toggle, hidden files toggle, command palette, empty directory.

**Gaps identified**:
- Local rename to an existing name (rejection → `rename-conflict` dialog). MTP rename rejection IS tested in `mtp.spec.ts`; local equivalent isn't.
- ⌘A / Ctrl+A select-all in pane (combined with F5 for multi-file copy).
- Cancel button on transfer dialog (only Escape tested).

**Tests added**: 0.

**Reason**: The existing spec covers the success path for every write operation end-to-end with byte-level assertions, plus negative cases for the structural flows (empty dir, view toggle). The rename-conflict dialog gap is real but adding it now risks duplicating the structurally-identical MTP rejection test for marginal coverage. The flow lives at the rename UI component level and is a good candidate for a tier-3 jsdom test rather than another full E2E round-trip.

### conflict-move.spec.ts / conflict-copy.spec.ts / conflict-edge-cases.spec.ts

**Source surface**: `src-tauri/src/file_system/write_operations/{copy,move}.rs` plus `transfer-conflict-policy` UI.

**Behaviors covered (before)** (17 across three files): Overwrite All, Skip All, per-file decisions, Rename, Rename All, Layout A nested conflicts, Layout B multi-item merges, mid-operation rollback, sequential conflicts, symlinks, type mismatches (file↔dir).

**Gaps identified**:
- Same-volume copy with both conflict and non-conflict mixed (already covered by Layout A/B).
- ⌃Z/⌘Z to undo a completed transfer — but the app doesn't have undo today.

**Tests added**: 0. **Reason**: The conflict-policy matrix is saturated by the three files together. Adding more cases dilutes signal.

### app.spec.ts

**Source surface**: `src/routes/(main)/+page.svelte` and the global keyboard dispatch (`command-dispatch.ts`, `command-registry.ts`).

**Behaviors covered (before)** (14): Render, dual pane, file entries, arrow nav, Tab pane switch, Space toggle, click cursor move, click pane focus, Enter into dir, Backspace to parent, F7 mkdir dialog open/cancel, F7 mkdir create, F5 copy dialog open/escape, F6 move dialog open/escape.

**Gaps identified**:
- F8 opens delete confirmation (vs. ⇧F8 which is permanent delete).
- Cancel **button** on the mkdir dialog (was only tested via Escape).
- Cancel button on transfer dialogs.
- ⌘A select-all keyboard flow.

**Tests added** (2):
1. `Cancel button closes the new folder dialog without creating anything` — exercises the `.btn-secondary` path through `ModalDialog`, asserts no folder was created (file-entry count unchanged).
2. `opens the delete confirmation dialog with F8` — F8 opens the `delete-confirmation` dialog (the recycle-bin path, not ⇧F8); Escape closes it and leaves the file under cursor in place.

**Skipped (with reason)**: ⌘A and Cancel-button on the transfer dialog — the transfer-dialog Cancel button is wired through the same path as Escape (both call the same `closeDialog`), so the additional test would duplicate signal.

### accessibility.spec.ts

**Source surface**: `src/lib/test-a11y.ts` runner + axe-core rules across each dialog snapshot in light + dark.

**Behaviors covered (before)** (20): Main explorer, every major dialog (Copy/Delete/Move/About/License/Command palette/Search/Settings/File viewer) in both light and dark modes.

**Gaps identified**:
- Error pane in axe — but `error-pane.spec.ts` covers ARIA explicitly.
- Network volume picker open state — narrower coverage.

**Tests added**: 0. **Reason**: Each frame is already audited in both modes; structural a11y for individual components lives at tier 3 (`*.a11y.test.ts`). Adding more axe snapshots without a clear missing-component would inflate the suite.

### file-watching.spec.ts

**Source surface**: `src-tauri/src/file_system/watch/` + the frontend watcher subscription in `file-explorer`.

**Behaviors covered (before)** (11): External create (file + dir), delete, rename, modify-size, batch (25), 600-threshold (Linux only), watched-dir deletion, dual-pane sync, in-app-copy dedup, hidden-file filtering.

**Gaps identified**:
- Permissions-change watching — out of e2e scope.
- Watcher behavior under symlink resolution.

**Tests added**: 0. **Reason**: The spec already covers the full CRUD matrix plus the structural edge cases (threshold, dedup, hidden-file filtering, watched-dir deletion). The remaining gaps are too low-level for an E2E round-trip and would belong in `notify`-level Rust tests.

### mtp.spec.ts / mtp-conflicts.spec.ts

**Behaviors covered (before)** (26 total): Volume picker, browse, free space, copy bidirectional, move within and across, delete (single, multi, recursive), mkdir, rename, rename rejection, read-only enforcement, Cmd+C/X/V rejection toasts, 50 MB transfer in both directions, external add detection, MTP-to-local and same-volume conflict matrix (overwrite/skip).

**Gaps identified**:
- MTP rename to dotfile (filesystem reserved-name handling): valid but feels nichey.
- MTP filename Unicode round-trip: covered indirectly by the SMB unicode tests (skipped on macOS).

**Tests added**: 0. **Reason**: 26 tests cover the user-observable surface end-to-end. Adding marginal cases would test virtual-device internals rather than user flows.
