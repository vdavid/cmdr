# Test coverage improvement notes

## Session: 2026-01-16

### What was done

1. **Created ESLint rule `no-isolated-tests`** (`apps/desktop/eslint-plugins/no-isolated-tests.js`)
   - Flags test files that don't import any source code
   - Prevents tautological tests that only test local mocks
   - Applied to `src/**/*.test.ts` (excludes e2e tests)

2. **Set up code coverage**
   - Added `@vitest/coverage-v8` dependency
   - Configured `vitest.config.ts` with json-summary output
   - Threshold: 70% line coverage
   - Allowlist: `coverage-allowlist.json` for files exempt from threshold

3. **Updated `svelte_checks.go`**
   - Tests now run with coverage (`pnpm test:coverage`)
   - Parses `coverage/coverage-summary.json`
   - Checks each file against 70% threshold
   - Respects allowlist with reasons

4. **Deleted tautological test files** (5 files, ~700+ lines)
   - `network-hosts.test.ts` - tested local mocks only
   - `keyboard-navigation.test.ts` - tested local functions
   - `network-auth.test.ts` - tested local mocks
   - `network-mount.test.ts` - tested local mocks
   - `volume-paths.test.ts` - tested local functions

5. **Added real tests for utility files**
   - `file-list-utils.test.ts` - 39 tests, 100% coverage
   - `keyboard-shortcuts.test.ts` - 23 tests, 100% coverage

### Current coverage status

Files now at 100%:
- `file-list-utils.ts`
- `keyboard-shortcuts.ts`
- `navigation-history.ts`
- `apply-diff.ts`
- `fuzzy-search.ts`
- `command-registry.ts`

Files close to threshold (need small boost):
- `BriefList.svelte` - 68.05% (need ~2% more)
- `virtual-scroll.ts` - 88.23%

Files needing significant work (currently allowlisted):
- `FullList.svelte` - 33.06%
- `FilePane.svelte` - 41.29%
- `DualPaneExplorer.svelte` - 22.89%
- `SelectionInfo.svelte` - 17.27%

### Files in allowlist (with reasons)

**Tauri API dependent** (hard to unit test):
- `tauri-commands.ts`, `app-status-store.ts`, `icon-cache.ts`, `drag-drop.ts`
- `settings-store.ts`, `licensing-store.svelte.ts`, `network-store.svelte.ts`
- `updater.svelte.ts`, `window-state.ts`

**Network components** (need integration tests):
- `NetworkBrowser.svelte`, `NetworkLoginForm.svelte`, `ShareBrowser.svelte`

**Simple UI components**:
- `UpdateNotification.svelte`, `PermissionDeniedPane.svelte`
- `licensing/*.svelte`, `onboarding/*.svelte`

**Dev tooling**:
- `benchmark.ts`, `font-metrics/measure.ts`

### Next steps

1. Remove `file-list-utils.ts` and `keyboard-shortcuts.ts` from allowlist (done - 100%)
2. Add more tests to `BriefList.svelte` to push over 70%
3. Add tests for `FullList.svelte`
4. Add tests for `SelectionInfo.svelte`
5. Consider if large components (FilePane, DualPaneExplorer) can be tested better

### Testing philosophy

- Test behavior, not implementation
- Focus on edge cases and boundary conditions
- Pure functions are easy to test - prioritize those
- Component tests should verify user-visible behavior
- Avoid testing framework code or trivial getters/setters
