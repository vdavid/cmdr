/**
 * The words and facts a group row reads out: which action the button names, why an approval
 * can or can't be taken back (and what it runs as instead), and what a file's size and date say now against what they said
 * when the AI suggested it.
 *
 * Pure, so the copy each verb gets and the comparison each row makes are pinned by tests rather
 * than by eyeballing a dialog.
 */

import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { ProposalVerb, Reversibility, RunsAs } from '#lib/ipc/bindings.js'
import type { SuggestedOpView } from '#lib/tauri-commands/index.js'
import type { LiveFact } from './suggested-ops-trigger.svelte'

/** The primary button for each verb, taking `{count}`. Named per verb so the button says what
 *  happens ("Move 4 files"), and stays the same whether or not the list is scrolled. */
export function approveKey(verb: ProposalVerb): MessageKey {
  switch (verb) {
    case 'move':
      return 'suggestedOps.approve.move'
    case 'copy':
      return 'suggestedOps.approve.copy'
    case 'trash':
      return 'suggestedOps.approve.trash'
    case 'delete':
      return 'suggestedOps.approve.delete'
    case 'rename':
      return 'suggestedOps.approve.rename'
    case 'compress':
      return 'suggestedOps.approve.compress'
    case 'extract':
      return 'suggestedOps.approve.extract'
    case 'addServer':
      return 'suggestedOps.approve.addServer'
    case 'editServer':
      return 'suggestedOps.approve.editServer'
  }
}

/**
 * The reversibility pill, saying WHY, taking `{count}`.
 *
 * The verb picks the reason: the same `restoreMove` means "it goes to the Trash" for a trash
 * group and "Cmdr can move them back" for a move.
 */
export function reversibilityKey(verb: ProposalVerb, reversible: Reversibility): MessageKey {
  switch (reversible) {
    case 'restoreMove':
      if (verb === 'trash') return 'suggestedOps.reversible.trash'
      if (verb === 'rename') return 'suggestedOps.reversible.renameBack'
      return 'suggestedOps.reversible.moveBack'
    case 'deleteWhatWasWritten':
      if (verb === 'compress') return 'suggestedOps.reversible.removeArchive'
      if (verb === 'extract') return 'suggestedOps.reversible.removeExtracted'
      return 'suggestedOps.reversible.removeCopies'
    case 'irreversible':
      if (verb === 'delete') return 'suggestedOps.irreversible.delete'
      if (verb === 'compress') return 'suggestedOps.irreversible.overwrite'
      return 'suggestedOps.irreversible.other'
    case 'serverSettings':
      if (verb === 'editServer') return 'suggestedOps.reversible.changeServerBack'
      return 'suggestedOps.reversible.forgetServer'
  }
}

/**
 * The action that would really run when the user approves: the verb, or the fallback the review
 * offers instead (`runsAs`). Every word the group says about its action (its title, its button,
 * its undo note) reads from this, so a trash on a drive with no Trash says "Delete" everywhere.
 */
export function shownVerb(verb: ProposalVerb, runsAs: RunsAs | null): ProposalVerb {
  switch (runsAs) {
    case 'permanentDelete':
      return 'delete'
    case null:
      return verb
  }
}

/** The undo note's reversibility for the action that would really run (see `shownVerb`). */
export function shownReversibility(reversible: Reversibility, runsAs: RunsAs | null): Reversibility {
  return runsAs === 'permanentDelete' ? 'irreversible' : reversible
}

/** The pill and its tooltip saying why a group runs as something other than its verb. Keyed by
 *  every `RunsAs`, so a new variant is a compile error until it has its words. */
export function runsAsKeys(runsAs: RunsAs): { pill: MessageKey; tooltip: MessageKey } {
  return RUNS_AS_KEYS[runsAs]
}

const RUNS_AS_KEYS: Record<RunsAs, { pill: MessageKey; tooltip: MessageKey }> = {
  permanentDelete: { pill: 'suggestedOps.noTrash', tooltip: 'suggestedOps.noTrashTooltip' },
}

/** The action a group's title leads with, so what happens reads from the title alone. */
export function actionKey(verb: ProposalVerb): MessageKey {
  switch (verb) {
    case 'move':
      return 'suggestedOps.action.move'
    case 'copy':
      return 'suggestedOps.action.copy'
    case 'trash':
      return 'suggestedOps.action.trash'
    case 'delete':
      return 'suggestedOps.action.delete'
    case 'rename':
      return 'suggestedOps.action.rename'
    case 'compress':
      return 'suggestedOps.action.compress'
    case 'extract':
      return 'suggestedOps.action.extract'
    case 'addServer':
      return 'suggestedOps.server.add'
    case 'editServer':
      return 'suggestedOps.server.change'
  }
}

/**
 * What a row's Size and Last modified columns show.
 *
 * - `live`: Cmdr looked just now. `sizeChanged` / `modifiedChanged` say the value moved since the
 *   suggestion, so the row can show both.
 * - `missing`: the file isn't at its path any more.
 * - `snapshot`: Cmdr couldn't look (or hasn't yet), so this is what the file was when the AI
 *   suggested it, and the row labels it that way. Either value may be `null` when the
 *   suggestion recorded nothing, which the row says in words, never as a zero.
 */
export type RowFacts =
  | {
      kind: 'live'
      /** `null` for a folder, whose own stat size means nothing to a user. */
      size: number | null
      modified: number | null
      snapshotSize: number | null
      snapshotModified: number | null
      sizeChanged: boolean
      modifiedChanged: boolean
    }
  | { kind: 'missing' }
  | { kind: 'snapshot'; size: number | null; modified: number | null }

export function rowFacts(op: SuggestedOpView, live: LiveFact | undefined): RowFacts {
  if (live?.kind === 'present') {
    return {
      kind: 'live',
      size: live.size,
      modified: live.modified,
      snapshotSize: op.snapshotSize,
      snapshotModified: op.snapshotModified,
      // Nothing recorded at suggestion time means nothing to have changed FROM.
      sizeChanged: op.snapshotSize !== null && live.size !== null && op.snapshotSize !== live.size,
      modifiedChanged: op.snapshotModified !== null && live.modified !== null && op.snapshotModified !== live.modified,
    }
  }
  if (live?.kind === 'missing') return { kind: 'missing' }
  return { kind: 'snapshot', size: op.snapshotSize, modified: op.snapshotModified }
}

/** The last path segment: the name a row leads with. */
export function fileName(path: string): string {
  const trimmed = path.endsWith('/') && path.length > 1 ? path.slice(0, -1) : path
  return trimmed.slice(trimmed.lastIndexOf('/') + 1)
}

/** Everything before the name, or `''` for a bare name. */
export function parentFolder(path: string): string {
  const trimmed = path.endsWith('/') && path.length > 1 ? path.slice(0, -1) : path
  const slash = trimmed.lastIndexOf('/')
  if (slash < 0) return ''
  return slash === 0 ? '/' : trimmed.slice(0, slash)
}

/** The icon id the shared icon cache keys a file by (`ext:pdf`, or `file` with no extension).
 *  Mirrors the backend's `derive_icon_id`; an op is a file as far as the row can tell. */
export function fileIconId(path: string): string {
  const name = fileName(path)
  const dot = name.lastIndexOf('.')
  if (dot <= 0 || dot === name.length - 1) return 'file'
  return `ext:${name.slice(dot + 1).toLowerCase()}`
}

/** How long ago a sweep was suggested, in the words its heading uses. */
export type SweepWhen =
  | { kind: 'justNow' }
  | { kind: 'minutesAgo'; minutes: number }
  | { kind: 'today' }
  | { kind: 'yesterday' }
  | { kind: 'earlier' }

/**
 * A sweep's age as the heading says it: "just now" under a minute, minutes for the first hour,
 * then "today at" / "yesterday at" a time, and the full date for anything older. Calendar days
 * are local, so "yesterday" means the user's yesterday, not 24 hours.
 */
export function sweepWhen(createdAtSeconds: number, nowMs: number): SweepWhen {
  const ageMs = nowMs - createdAtSeconds * 1000
  if (ageMs < 60_000) return { kind: 'justNow' }
  if (ageMs < 60 * 60_000) return { kind: 'minutesAgo', minutes: Math.floor(ageMs / 60_000) }
  const created = new Date(createdAtSeconds * 1000)
  const startOfToday = new Date(nowMs)
  startOfToday.setHours(0, 0, 0, 0)
  if (created >= startOfToday) return { kind: 'today' }
  const startOfYesterday = new Date(startOfToday)
  startOfYesterday.setDate(startOfYesterday.getDate() - 1)
  if (created >= startOfYesterday) return { kind: 'yesterday' }
  return { kind: 'earlier' }
}

/** How recent a lone sweep has to be for its heading to say nothing worth a line. */
export const FRESH_SWEEP_MS = 10 * 60_000

/** A single sweep from the last ten minutes needs no heading: the user just asked for it. */
export function sweepHeadingHidden(sweepCount: number, createdAtSeconds: number, nowMs: number): boolean {
  return sweepCount === 1 && nowMs - createdAtSeconds * 1000 < FRESH_SWEEP_MS
}
