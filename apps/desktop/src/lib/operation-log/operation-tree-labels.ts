/**
 * The words of the Operation log's item tree: each row's status and its longer
 * explanation, a folder's rollup, the route line, and why the tree has no rows yet.
 *
 * Pure, no I/O. Every word comes from a typed field of the backend's answer
 * (`OperationTreeRow`, `TreeTally`) through an exhaustive switch, ❌ never from a string
 * the backend rendered, so a new skip reason or unexpandable reason is a compile error
 * here rather than a blank cell.
 */

import type {
  OpKind,
  OperationTreeRow,
  SkipReason,
  TreeTally,
  UnbuiltReason,
  UnexpandableReason,
} from '#lib/ipc/bindings.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'
import { displayPath } from '#lib/utils/display-path.js'
import { rollbackConfirmVariant } from '#lib/file-operations/reversal-wording.js'
import type { TreeBlank } from './operation-tree-source.svelte'

/** How a status reads: what came of the row, for its glyph and color. */
export type TreeStatusTone = 'rolledBack' | 'skipped' | 'unfinished' | 'rollup'

export interface TreeRowStatus {
  word: string
  tone: TreeStatusTone
}

/**
 * Whether the tree shows an operation where its things LANDED (copies, moves, new
 * items) or where they WERE (a trash, a delete, a rename, a tag change). Mirrors the
 * backend's `TreeAxis::of`, which picks the side the undo acts on.
 */
function showsLandedSide(kind: OpKind): boolean {
  switch (kind) {
    case 'copy':
    case 'move':
    case 'createFile':
    case 'createFolder':
    case 'archiveEdit':
      return true
    case 'trash':
    case 'delete':
    case 'rename':
    case 'tag':
      return false
  }
}

/** A rolled-back row's word, by what the reversal did to it. */
function rolledBackWord(kind: OpKind): string {
  switch (rollbackConfirmVariant(kind)) {
    case 'undoByDeleting':
    case 'stopAndDelete':
      return tString('operationLog.tree.statusRemoved')
    case 'undoByMovingBack':
    case 'stopAndMoveBack':
      return tString('operationLog.tree.statusMovedBack')
    case 'undoByRenamingBack':
      return tString('operationLog.tree.statusRenamedBack')
    case 'undoByRestoringTags':
      return tString('operationLog.tree.statusTagsRestored')
  }
}

/**
 * The status cell, or `null` when the row has nothing worth saying. It speaks only when
 * it matters: rolled back, skipped, didn't finish, and on a folder, how much under it
 * came back or was skipped. A row still waiting to be rolled back says nothing; its
 * checkbox already does.
 */
export function treeRowStatus(row: OperationTreeRow, kind: OpKind): TreeRowStatus | null {
  switch (row.outcome) {
    case 'rolledBack':
      return { word: rolledBackWord(kind), tone: 'rolledBack' }
    case 'skipped':
      return { word: tString('operationLog.tree.statusSkipped'), tone: 'skipped' }
    case 'failed':
      return { word: tString('operationLog.tree.statusUnfinished'), tone: 'unfinished' }
    case 'done':
    case null:
      return row.tally === null ? null : folderRollup(row.tally)
  }
}

/**
 * "3 rolled back, 2 skipped" for a folder, from its counts; `null` when nothing under it
 * moved. The two counts join through `operationLog.tree.rollupBoth`, so each language
 * picks its own separator (Chinese wants "，"), ❌ never a hardcoded ", ".
 */
function folderRollup(tally: TreeTally): TreeRowStatus | null {
  const rolledBack =
    tally.rolledBack > 0
      ? tString('operationLog.tree.rollupRolledBack', {
          count: tally.rolledBack,
          countText: formatInteger(tally.rolledBack),
        })
      : null
  const skipped =
    tally.heldBack > 0
      ? tString('operationLog.tree.rollupSkipped', { count: tally.heldBack, countText: formatInteger(tally.heldBack) })
      : null
  if (rolledBack !== null && skipped !== null) {
    return { word: tString('operationLog.tree.rollupBoth', { rolledBack, skipped }), tone: 'rollup' }
  }
  const word = rolledBack ?? skipped
  return word === null ? null : { word, tone: 'rollup' }
}

/**
 * The row's longer story, in sentences: why it was skipped, that it comes back with its
 * folder, why a folder can't unfold, where its other end is. The cursor row shows it,
 * and it's every row's accessible description, so a keyboard or screen-reader user
 * hears what a pointer user would hover for. Empty when there's nothing to add.
 */
export function treeRowTips(row: OperationTreeRow, kind: OpKind): string[] {
  const tips: string[] = []
  if (row.role === 'contents') tips.push(tString('operationLog.tree.comesBackWithFolder'))
  if (row.skipReason !== null) tips.push(tString(skipReasonNotice(row.skipReason)))
  else if (row.outcome === 'skipped') tips.push(tString('operationLog.tree.operationSkipped'))
  if (row.outcome === 'failed') tips.push(tString('operationLog.tree.operationUnfinished'))
  if (row.unexpandable !== null) tips.push(tString(unexpandableNotice(row.unexpandable)))
  if (row.otherEnd !== null) {
    const path = displayPath(row.otherEnd)
    tips.push(
      showsLandedSide(kind)
        ? tString('operationLog.tree.otherEndFrom', { path })
        : tString('operationLog.tree.otherEndTo', { path }),
    )
  }
  return tips
}

/**
 * Why a rollback held this item back, one sentence per reason. ❌ Never "error" or
 * "failed": every one of these is Cmdr declining to touch something it couldn't
 * vouch for, which is the feature working.
 */
export function skipReasonNotice(reason: SkipReason): MessageKey {
  switch (reason) {
    case 'unverifiablePrecondition':
      return 'operationLog.skipReason.unverifiablePrecondition'
    case 'drift':
      return 'operationLog.skipReason.drift'
    case 'restoreTargetOccupied':
      return 'operationLog.skipReason.restoreTargetOccupied'
    case 'dirNotEmpty':
      return 'operationLog.skipReason.dirNotEmpty'
    case 'alreadyGone':
      return 'operationLog.skipReason.alreadyGone'
    case 'vanished':
      return 'operationLog.skipReason.vanished'
    case 'originalGone':
      return 'operationLog.skipReason.originalGone'
    case 'failed':
      return 'operationLog.skipReason.failed'
    case 'locked':
      return 'operationLog.skipReason.locked'
  }
}

/**
 * Why a folder can't unfold. `capped` speaks about the OPERATION, never this folder: the
 * journal records coverage per operation, so this folder may be an empty one beside
 * the big one.
 */
export function unexpandableNotice(reason: UnexpandableReason): MessageKey {
  switch (reason) {
    case 'empty':
      return 'operationLog.tree.unexpandableEmpty'
    case 'notRecorded':
      return 'operationLog.tree.unexpandableNotRecorded'
    case 'capped':
      return 'operationLog.tree.unexpandableCapped'
  }
}

/** A folder's size cell: how many files sit under it. Empty for a folder with none. */
export function folderSizeText(tally: TreeTally): string {
  if (tally.files === 0) return ''
  return tString('operationLog.tree.folderFiles', { count: tally.files, countText: formatInteger(tally.files) })
}

/**
 * The route line under the detail header ("From ~/Documents to /Volumes/Backup"), worded
 * by what the operation did, or `null` when the tree has no single root (a forest of
 * volumes) to name.
 */
export function operationRoute(kind: OpKind, rootPath: string | null, otherRoot: string | null): string | null {
  if (rootPath === null) return null
  const root = displayPath(rootPath)
  switch (kind) {
    case 'copy':
    case 'move':
      return otherRoot === null
        ? tString('operationLog.route.to', { to: root })
        : tString('operationLog.route.fromTo', { from: displayPath(otherRoot), to: root })
    case 'trash':
      return tString('operationLog.route.fromToTrash', { from: root })
    case 'delete':
      return tString('operationLog.route.from', { from: root })
    case 'rename':
    case 'tag':
    case 'createFile':
    case 'createFolder':
    case 'archiveEdit':
      return tString('operationLog.route.in', { path: root })
  }
}

/** Why the tree shows no rows, in one sentence. */
export function treeBlankNotice(blank: TreeBlank): MessageKey {
  switch (blank.kind) {
    case 'loading':
      return 'operationLog.dialog.loading'
    case 'backfilling':
      return 'operationLog.tree.backfilling'
    case 'indexesBuilding':
      return 'operationLog.tree.indexesBuilding'
    case 'indexesUnavailable':
      return 'operationLog.tree.indexesUnavailable'
    case 'unbuilt':
      return unbuiltNotice(blank.reason)
    case 'gone':
      return 'operationLog.rollback.refusalUnknown'
    case 'unreadable':
      return 'operationLog.tree.unreadable'
    case 'empty':
      return 'operationLog.dialog.noItems'
  }
}

function unbuiltNotice(reason: UnbuiltReason): MessageKey {
  switch (reason) {
    case 'notBuilt':
      return 'operationLog.tree.backfilling'
    case 'operationRunning':
      return 'operationLog.tree.operationRunning'
    case 'rollingBack':
      return 'operationLog.tree.rollingBack'
  }
}
