import { vi } from 'vitest'
import type { PaneAccess } from './pane-access'
import type { FilePaneAPI } from './types'
import type { SearchSnapshot } from '$lib/search/snapshot-store.svelte'
import type { FileEntry, VolumeInfo } from '../types'
import type { ToastContent, ToastOptions } from '$lib/ui/toast/toast-store.svelte'

/**
 * Shared stubs and fixtures for the `file-operation-commands` specs
 * (`file-operation-commands.test.ts` and `file-operation-commands.search-results.test.ts`).
 *
 * This module deliberately imports NOTHING but types and `vitest`. Each spec
 * registers its own `vi.mock` factories and reaches the spies below through a
 * lazy `await import(...)` inside the factory; if this module imported
 * `./file-operation-commands`, that factory would run while this one is still
 * evaluating and read `spies` from its temporal dead zone.
 */
export const spies = {
  getFileAt: vi.fn<() => Promise<FileEntry | null>>(),
  getFilesAtIndices: vi.fn<() => Promise<FileEntry[]>>(),
  addToast: vi.fn<(content: ToastContent, options?: ToastOptions) => string>(),
  getSnapshot: vi.fn<() => SearchSnapshot | undefined>(),
  openFileViewer: vi.fn<() => Promise<void>>(),
  getInitialFolderName: vi.fn<() => Promise<string>>(),
  getInitialFileName: vi.fn<() => Promise<string>>(),
  buildTransferPropsFromSelection: vi.fn<(...args: unknown[]) => Promise<unknown>>(),
  buildTransferPropsFromCursor: vi.fn<(...args: unknown[]) => Promise<unknown>>(),
  logWarn: vi.fn(),
  logDebug: vi.fn(),
}

/**
 * Stands in for the store's `resolveSnapshotEntries` (selection wins, cursor is
 * the fallback, out-of-range dropped) over whatever `spies.getSnapshot` returns.
 * The real one is unit-tested in `search/snapshot-store.svelte.ts.test.ts`; this
 * copy keeps the opener specs about WHICH rows an opener acts on.
 */
export function resolveSnapshotEntriesStub(
  _id: string,
  selectedIndices: number[],
  cursorIndex: number,
): SearchSnapshot['entries'] {
  const snap = spies.getSnapshot()
  if (!snap) return []
  const indices = selectedIndices.length > 0 ? selectedIndices : [cursorIndex]
  return indices.flatMap((i) => (i >= 0 && i < snap.entries.length ? [snap.entries[i]] : []))
}

/** Builds a `FilePaneAPI` stub exposing only the members the file-operation band reads. */
export function buildPaneRef(
  overrides: Partial<{
    listingId: string | null
    volumeId: string
    hasParent: boolean
    selectedIndices: number[]
    cursorIndex: number
    currentPath: string
    startRename: () => void
    cancelRename: () => void
    isRenaming: () => boolean
  }> = {},
): FilePaneAPI {
  const stub = {
    getListingId: () => ('listingId' in overrides ? overrides.listingId : 'listing-1'),
    getVolumeId: () => overrides.volumeId ?? 'root',
    hasParentEntry: () => overrides.hasParent ?? false,
    getSelectedIndices: () => overrides.selectedIndices ?? [],
    getCursorIndex: () => overrides.cursorIndex ?? 0,
    getCurrentPath: () => overrides.currentPath ?? '/Users/x/dir',
    startRename: overrides.startRename ?? vi.fn(),
    cancelRename: overrides.cancelRename ?? vi.fn(),
    isRenaming: overrides.isRenaming ?? (() => false),
  }
  return stub as unknown as FilePaneAPI
}

interface AccessConfig {
  focusedPane?: 'left' | 'right'
  paneRefs?: Partial<Record<'left' | 'right', FilePaneAPI | undefined>>
  volumeIds?: Partial<Record<'left' | 'right', string>>
  paths?: Partial<Record<'left' | 'right', string>>
  volumes?: VolumeInfo[]
  showHiddenFiles?: boolean
  focusContainer?: () => void
}

export function buildAccess(config: AccessConfig = {}): PaneAccess {
  const otherPane = (pane: 'left' | 'right'): 'left' | 'right' => (pane === 'left' ? 'right' : 'left')
  const defaultRef = buildPaneRef()
  return {
    getPaneRef: (pane) => (config.paneRefs && pane in config.paneRefs ? config.paneRefs[pane] : defaultRef),
    getPanePath: (pane) => config.paths?.[pane] ?? (pane === 'left' ? '/left/dir' : '/right/dir'),
    getPaneVolumeId: (pane) => config.volumeIds?.[pane] ?? 'root',
    getPaneSort: () => ({ sortBy: 'name', sortOrder: 'ascending' }),
    getPaneHistory: () => ({ stack: [], currentIndex: 0 }),
    getFocusedPane: () => config.focusedPane ?? 'left',
    otherPane,
    getShowHiddenFiles: () => config.showHiddenFiles ?? true,
    getVolumes: () => config.volumes ?? [],
    focusContainer: config.focusContainer ?? (() => {}),
  }
}

export interface DialogsStub {
  showAlert: ReturnType<typeof vi.fn>
  showNewFolder: ReturnType<typeof vi.fn>
  showNewFile: ReturnType<typeof vi.fn>
  showTransfer: ReturnType<typeof vi.fn>
  showDeleteConfirmation: ReturnType<typeof vi.fn>
  closeConfirmationDialog: ReturnType<typeof vi.fn>
  isConfirmationDialogOpen: ReturnType<typeof vi.fn>
}

export function buildDialogs(): DialogsStub {
  return {
    showAlert: vi.fn(),
    showNewFolder: vi.fn(),
    showNewFile: vi.fn(),
    showTransfer: vi.fn(),
    showDeleteConfirmation: vi.fn(),
    closeConfirmationDialog: vi.fn(),
    isConfirmationDialogOpen: vi.fn(() => false),
  }
}

/** A minimal VolumeInfo with overridable flags. */
export function volume(overrides: Partial<VolumeInfo> = {}): VolumeInfo {
  return {
    id: 'root',
    name: 'Macintosh HD',
    mountIsReadOnly: false,
    supportsTrash: true,
    ...overrides,
  } as unknown as VolumeInfo
}

export function snapshotEntry(
  overrides: Partial<SearchSnapshot['entries'][number]> = {},
): SearchSnapshot['entries'][number] {
  return {
    name: 'doc.txt',
    path: '/real/dir/doc.txt',
    parentPath: '/real/dir',
    isDirectory: false,
    size: 42,
    modifiedAt: null,
    iconId: 'ext:txt',
    ...overrides,
  }
}

/** A snapshot of `entries`, found by a search that covered `volumeId`. */
export function snapshot(entries: SearchSnapshot['entries'], volumeId = 'root'): SearchSnapshot {
  return { entries, volumeId } as unknown as SearchSnapshot
}

export function fileEntry(overrides: Partial<FileEntry> = {}): FileEntry {
  return {
    name: 'doc.txt',
    path: '/Users/x/dir/doc.txt',
    isDirectory: false,
    isSymlink: false,
    size: 10,
    recursiveSize: undefined,
    recursiveFileCount: undefined,
    ...overrides,
  } as unknown as FileEntry
}
