/**
 * Test fixtures for the Operation log's item tree: rows and windows shaped like
 * `get_operation_log_tree`'s answers, so the source and the dialog are tested against
 * the wire shape rather than a hand-rolled stand-in.
 */

import type { OperationTreeRow, OperationTreeView, TreeTally } from '#lib/ipc/bindings.js'

export function tally(over: Partial<TreeTally> = {}): TreeTally {
  return {
    files: 0,
    bytes: 0,
    selected: 0,
    selectable: 0,
    rolledBack: 0,
    heldBack: 0,
    unfinished: 0,
    housekeepingWaiting: 0,
    ...over,
  }
}

/** A file row, keyed by its seq. */
export function fileRow(seq: number, over: Partial<OperationTreeRow> = {}): OperationTreeRow {
  return {
    index: 0,
    key: { kind: 'item', seq },
    label: `file-${String(seq)}.txt`,
    level: 1,
    entryType: 'file',
    expandable: false,
    expanded: false,
    posInSet: 1,
    setSize: 1,
    role: 'selectable',
    outcome: 'done',
    skipReason: null,
    checkbox: 'on',
    size: 1_024,
    tally: null,
    fullPath: `/dest/file-${String(seq)}.txt`,
    otherEnd: null,
    unexpandable: null,
    ...over,
  }
}

/** A folder row, keyed by its dir id. */
export function folderRow(dirId: number, over: Partial<OperationTreeRow> = {}): OperationTreeRow {
  return {
    index: 0,
    key: { kind: 'folder', dirId },
    label: `folder-${String(dirId)}`,
    level: 1,
    entryType: 'dir',
    expandable: true,
    expanded: false,
    posInSet: 1,
    setSize: 1,
    role: null,
    outcome: null,
    skipReason: null,
    checkbox: 'on',
    size: null,
    tally: tally({ files: 2, selected: 2, selectable: 2 }),
    fullPath: `/dest/folder-${String(dirId)}`,
    otherEnd: null,
    unexpandable: null,
    ...over,
  }
}

/** A built window over `rows` (their `index` filled in from `offset`). */
export function treeView(rows: OperationTreeRow[], over: Partial<OperationTreeView> = {}): OperationTreeView {
  const offset = over.offset ?? 0
  return {
    summary: { kind: 'built' },
    readOnly: false,
    rootPath: '/dest',
    otherRoot: '/src',
    totalRows: offset + rows.length,
    offset,
    rows: rows.map((row, i) => ({ ...row, index: offset + i })),
    ancestors: [],
    cursorIndex: null,
    tally: tally({ files: rows.length, selected: rows.length, selectable: rows.length }),
    selection: { base: 'everything', toggles: [] },
    togglesCapped: false,
    ...over,
  }
}
