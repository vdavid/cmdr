/**
 * What a file row says to a screen reader, shared by `BriefList` and `FullList` so the two
 * views can't drift: the option's id, accessible name, marked state, and place in the list,
 * plus which rendered row `aria-activedescendant` may name.
 *
 * The listbox is `aria-multiselectable`: `aria-selected` on an option means MARKED, and the
 * cursor is only the listbox's `aria-activedescendant` (and the `is-under-cursor` class).
 *
 * Everything here runs per rendered row per render, so it stays a plain function of the row's
 * data: no DOM reads, no formatting the visible cells already did (the caller hands in their
 * strings). Cost is bounded by the virtual window, never by the folder's size.
 */

import type { FileEntry, SyncStatus } from '../types'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { formatUnitList } from '#lib/intl/list-format.js'
import { getSyncStatusLabelKey } from './file-list-utils'
import { LOWER_BOUND_GLYPH, type DirSizeDisplayState } from './full-list-utils'
import type { CursorRowRef } from './list-focus.svelte'
import { rowDatePhraseKey, type DateColumn } from '../displayed-date'

/**
 * A file row's DOM id. `listId` is the list instance's `$props.id()`, so the two panes (and a
 * search-results list) never render the same id: `aria-activedescendant` resolves through
 * `getElementById`, which would hand a focused RIGHT list the LEFT pane's row.
 */
export function fileRowId(listId: string, index: number): string {
  return `${listId}-row-${String(index)}`
}

/** The kind word a row's name carries. A plain file has none: its extension is in its name. */
export type FileRowKind = 'folder' | 'symlink' | 'package' | 'archive'

const KIND_KEYS: Record<FileRowKind, MessageKey> = {
  folder: 'fileExplorer.list.rowKind.folder',
  symlink: 'fileExplorer.list.rowKind.symlink',
  package: 'fileExplorer.list.rowKind.package',
  archive: 'fileExplorer.list.rowKind.archive',
}

/**
 * The row's kind, from typed fields only (❌ never the `pkg:` prefix of `iconId`). A symlink
 * says "symlink" even when it points at a folder, the way its icon badge does.
 */
export function fileRowKind(
  entry: Pick<FileEntry, 'isDirectory' | 'isSymlink' | 'isPackage' | 'isArchive'>,
): FileRowKind | undefined {
  if (entry.isSymlink) return 'symlink'
  if (entry.isPackage === true) return 'package'
  if (entry.isDirectory) return 'folder'
  if (entry.isArchive === true) return 'archive'
  return undefined
}

/** The synthetic `..` row (`createParentEntry`). No real entry can carry that name: POSIX reserves it. */
function isParentRow(entry: FileEntry): boolean {
  return entry.name === '..'
}

export interface FileRowNameInput {
  entry: FileEntry
  /** The row's cloud-sync state; its badge's meaning joins the name. */
  syncStatus?: SyncStatus
  /** Whether macOS keeps Cmdr out of this folder; its glyph's meaning joins the name. */
  isRestricted?: boolean
  /**
   * The Full view's cells, as the row already formatted them. An empty or missing value is
   * left out (a folder whose size isn't known yet says no size).
   */
  details?: { host?: string; size?: string; date?: { column: DateColumn; text: string }; gitStatus?: string }
}

/**
 * The row's accessible name: "{name} ({kind}, {badges})", then (Full view) size, the date cell
 * as "(modified) {date}" (or "(created)", "(added)": whichever date `displayedDateColumn` put in
 * the cell), and "{status} in git": "src (folder), 4,096, (modified) 2026-10-14 10:30,
 * Modified in git". The git phrase needs no parentheses: the Git cell draws its glyph from CSS,
 * so none of it is visible text, and "in git" keeps it from echoing the date's "modified".
 * `..` says "Parent folder" in place of its name, then its cells like any row, its size as
 * "(this folder) 36,860": that cell shows the CURRENT folder's total, and bare after "Parent
 * folder" it sounded like the parent's. A restic snapshot row's Ext cell shows the host its
 * backup ran on, so the name says "(Backed up from) testnas" right after the row's own name.
 *
 * It's set as the option's `aria-label`, which replaces the name the row's content would give,
 * so every badge that speaks through content (`FileIcon`'s sync badge, the restricted and
 * archived glyphs) has to be said here. The symlink badge is the kind word.
 *
 * ❗ WCAG 2.5.3 (label in name, axe `label-content-name-mismatch`): the row's visible words
 * must run through the name unbroken and in order, so voice control can say what it sees.
 * Words the name adds BETWEEN visible ones go in parentheses, which axe drops before it
 * compares and VoiceOver doesn't speak; the visible ones stay bare, the `<dir>` placeholder
 * included. Joined by {@link formatUnitList}, which never adds an "and".
 */
export function fileRowName({ entry, syncStatus, isRestricted = false, details }: FileRowNameInput): string {
  const parent = isParentRow(entry)
  const size = details?.size ?? ''
  const parts = [
    parent
      ? tString('fileExplorer.list.parentRowName')
      : withAside(entry.name, rowAsides(entry, syncStatus, isRestricted)),
    details?.host ? asideAround('fileExplorer.restic.snapshot.host', details.host) : '',
    parent && size !== '' ? asideAround('fileExplorer.list.parentRowSize', size) : size,
    details?.date?.text ? asideAround(rowDatePhraseKey(details.date.column), details.date.text) : '',
    details?.gitStatus ? tString('fileExplorer.list.rowGitStatus', { status: details.gitStatus }) : '',
  ]
  return formatUnitList(parts.filter((part) => part !== ''))
}

/** What the row's kind and badges mean: the words its name adds that nothing on the row writes out. */
function rowAsides(entry: FileEntry, syncStatus: SyncStatus | undefined, isRestricted: boolean): string[] {
  const kind = fileRowKind(entry)
  const syncKey = getSyncStatusLabelKey(syncStatus)
  return [
    kind ? tString(KIND_KEYS[kind]) : '',
    syncKey ? tString(syncKey) : '',
    isRestricted ? tString('fileExplorer.restrictedFolder.label') : '',
    entry.inColdStorage === true ? tString('fileExplorer.archivedFile.label') : '',
  ].filter((aside) => aside !== '')
}

function withAside(text: string, asides: string[]): string {
  return asides.length === 0 ? text : `${text} (${formatUnitList(asides)})`
}

/** Stands in for the visible value while its phrase is split, so a translation may put it anywhere. */
const VALUE_SLOT = '\u0000'

/** Each phrase {@link asideAround} splits, and the param its visible value fills. */
const ASIDE_SLOTS = {
  'fileExplorer.list.rowModified': 'date',
  'fileExplorer.list.rowCreated': 'date',
  'fileExplorer.list.rowAdded': 'date',
  'fileExplorer.list.parentRowSize': 'size',
  'fileExplorer.restic.snapshot.host': 'host',
} as const satisfies Partial<Record<MessageKey, string>>

/**
 * A catalog phrase around a visible cell value, with the phrase's own words in parentheses:
 * "modified {date}" gives "(modified) 2026-10-14 10:30", "this folder {size}" gives "(this
 * folder) 36,860". The value is on the row; the words aren't. The phrase stays whole for
 * translators, whatever side of the value their language puts the words on.
 */
function asideAround(key: keyof typeof ASIDE_SLOTS, value: string): string {
  return tString(key, { [ASIDE_SLOTS[key]]: VALUE_SLOT })
    .split(VALUE_SLOT)
    .map((words) => words.trim())
    .map((words) => (words === '' ? '' : `(${words})`))
    .join(` ${value} `)
    .trim()
}

/**
 * The Full view's size cell as words, from what the cell itself renders: its override word
 * (no access, a git portal fact), else its size spans joined, with the `≥` of a lower bound,
 * else the `<dir>` placeholder of a folder whose size isn't known yet (WCAG 2.5.3: the name
 * says what the row shows).
 */
export function rowSizeText(cell: {
  override?: string
  triads?: readonly { value: string }[]
  dirState?: DirSizeDisplayState
}): string | undefined {
  if (cell.override !== undefined) return cell.override
  if (cell.dirState === 'dir' || cell.dirState === 'scanning') return tString('fileExplorer.dirSize.dirPlaceholder')
  if (!cell.triads) return undefined
  const size = cell.triads.map((triad) => triad.value).join('')
  return cell.dirState === 'lower-bound' ? LOWER_BOUND_GLYPH + size : size
}

export interface FileRowAttributes {
  id: string
  'aria-label': string
  'aria-selected': boolean
  'aria-posinset': number
  'aria-setsize': number
}

/**
 * Every ARIA attribute a file row's `role="option"` carries; both views spread it.
 *
 * ❗ Every option, `..` included, carries `aria-posinset` and `aria-setsize`: VoiceOver invents
 * a position from the RENDERED siblings for an option without one, so a `..` left out read
 * "Parent folder (1 of 8)" beside "subfolder, folder (1 of 7)" (2026-10-15 VoiceOver spike),
 * and under virtualization the rendered count is wrong anyway. `aria-selected` is always
 * present, `false` included.
 */
export function fileRowAttributes(row: {
  listId: string
  globalIndex: number
  totalCount: number
  isMarked: boolean
  name: string
}): FileRowAttributes {
  return {
    id: fileRowId(row.listId, row.globalIndex),
    'aria-label': row.name,
    'aria-selected': row.isMarked,
    'aria-posinset': row.globalIndex + 1,
    'aria-setsize': row.totalCount,
  }
}

/**
 * The cursor row, when it's rendered: what the listbox's `aria-activedescendant` may name.
 *
 * The cursor can point at nothing rendered: an empty folder, or a row scrolled outside the
 * virtual window. A dangling reference isn't harmless there: it's a critical a11y violation
 * (axe `aria-valid-attr-value`) and leaves screen readers announcing a stale row.
 */
export function renderedCursorRow(
  listId: string,
  cursorIndex: number,
  rows: Iterable<{ file: FileEntry; globalIndex: number }>,
): CursorRowRef | undefined {
  for (const row of rows) {
    if (row.globalIndex === cursorIndex) return { id: fileRowId(listId, cursorIndex), key: row.file.path }
  }
  return undefined
}
