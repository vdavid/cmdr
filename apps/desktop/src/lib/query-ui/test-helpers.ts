/**
 * Shared unit-test fixtures for the QueryDialog controller modules.
 *
 * `query-runner.svelte.ts`, `result-actions.ts`, and friends all take a `QueryDialogConfig`.
 * Building one by hand is ~60 lines of no-op callbacks, so this factory supplies a minimal
 * Search-shaped config with a real `createQueryFilterState()` instance, and each test
 * overrides only the fields it cares about.
 *
 * Component tests keep their own richer fixture inside `QueryDialog.svelte.test.ts`: it
 * records call transcripts for the mounted-dialog assertions, which the module tests don't
 * need.
 */

import type { SearchResultEntry } from '$lib/tauri-commands'
import type { QueryDialogConfig } from './query-dialog-config'
import { createQueryFilterState } from './query-filter-state.svelte'

/** A single stand-in result row. Tests that need several spread this with a fresh `path`. */
export const SAMPLE_ENTRY: SearchResultEntry = {
  name: 'photo.jpg',
  path: '/Users/test/photo.jpg',
  parentPath: '/Users/test',
  isDirectory: false,
  size: 1000,
  modifiedAt: 1_700_000_000,
  iconId: 'ext:jpg',
}

/** Builds `n` distinct result rows so cursor-index assertions can tell them apart. */
export function sampleEntries(n: number): SearchResultEntry[] {
  return Array.from({ length: n }, (_, i) => ({
    ...SAMPLE_ENTRY,
    name: `file-${String(i)}.jpg`,
    path: `/Users/test/file-${String(i)}.jpg`,
  }))
}

/**
 * A minimal Search-shaped `QueryDialogConfig`. Every callback is a no-op and every gate is
 * open (AI on, index ready, inputs enabled), so a test only has to override what it pins.
 */
export function makeQueryDialogConfig(overrides: Partial<QueryDialogConfig> = {}): QueryDialogConfig {
  const config: QueryDialogConfig = {
    title: 'Test dialog',
    dialogType: 'search',
    width: 'min(800px, 80vw)',
    state: createQueryFilterState({ defaultMode: 'filename' }),
    aiEnabled: true,
    inputsDisabled: false,
    visibleChips: { size: true, date: true, scope: true, pattern: true },
    showPathColumn: true,
    runHintCopy: 'Press Enter to search',
    historyStore: {
      getList: () => [],
      getLoaded: () => true,
      resetForTests: () => {},
      setList: () => {},
      load: () => Promise.resolve(),
    },
    recentItems: {
      adapter: () => ({ label: '', tooltip: '', mode: 'filename', ageLabel: '', metaLabel: '', ariaLabel: '' }),
      keyFn: () => '',
    },
    emptyState: { examples: [] },
    filterChipsExtras: {
      caseSensitive: false,
      scope: '',
      excludeSystemDirs: true,
      scopePresets: { currentFolder: '/Users/test', currentFolderUnavailableReason: '', volumeRoot: '/' },
      defaultScope: { path: '/Users/test', label: 'Current folder' },
      systemDirExcludeTooltip: '',
      aiPattern: null,
      aiPatternKind: null,
      onToggleCaseSensitive: () => {},
      onToggleExcludeSystemDirs: () => {},
      onSetScope: () => {},
      onClearAiPattern: () => {},
    },
    scanning: false,
    entriesScanned: 0,
    indexEntryCount: 1000,
    isIndexAvailable: true,
    isIndexReady: true,
    runQuery: () => Promise.resolve({ entries: [], totalCount: 0 }),
    onPickPath: () => {},
    onPickExample: () => {},
    onRowMenu: () => {},
    onActivateRecent: () => {},
    onRemoveRecent: () => {},
    onClose: () => {},
  }
  return { ...config, ...overrides }
}
