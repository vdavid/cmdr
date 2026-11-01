/**
 * Pure readings of a tool line's typed facts (`ToolOutcome`, `ToolSubject`), shared by the live
 * stream, the history fold, and the line itself, so all three read a line the same way.
 */

import type { ToolOutcome } from '#lib/ipc/bindings.js'
import type { RailToolCall } from './ask-cmdr-messages'

/** Which outcomes got what the call asked for. A `Record` over every kind, so a new outcome
 * variant is a compile error here rather than a silent guess. Mirrors `ToolOutcome::ok`. */
const OUTCOME_OK: Record<Exclude<ToolOutcome['kind'], 'connection'>, boolean> = {
  items: true,
  matches: true,
  nothingFound: true,
  answered: true,
  indexUnavailable: true,
  staged: true,
  notStaged: false,
  awaitingPerson: true,
  saved: true,
  refused: false,
  locationMiss: false,
}

/**
 * Whether a call got what it asked for: a refusal, a plan that staged nothing, a place that
 * couldn't be read, and a volume that didn't end up connected didn't.
 */
export function outcomeOk(outcome: ToolOutcome): boolean {
  if (outcome.kind === 'connection') return outcome.outcome === 'connected' || outcome.outcome === 'alreadyConnected'
  return OUTCOME_OK[outcome.kind]
}

/** A path a line names, as a place a pane can go: a folder to open, or a file to land on. */
export interface PathTarget {
  path: string
  kind: 'file' | 'folder'
}

/** Shows a line's path in a pane. One object payload, so an implementation can't bind the wrong slot. */
export type RevealPath = (target: PathTarget) => Promise<void>

/** What a line shows for a path: its last segment. The root, which has none, shows as itself. */
export function placeName(path: string): string {
  const trimmed = path.length > 1 && path.endsWith('/') ? path.slice(0, -1) : path
  const name = trimmed.slice(trimmed.lastIndexOf('/') + 1)
  return name.length > 0 ? name : trimmed
}

/** Whether a line wears the "same as above" tag: it repeats an earlier call of its turn. The
 * breaker's own refusal already says so in its outcome, so it doesn't get the tag twice. */
export function showsRepeatTag(call: RailToolCall): boolean {
  if (call.repeatOf === null) return false
  return !(call.outcome?.kind === 'refused' && call.outcome.reason.kind === 'repeatedCall')
}
