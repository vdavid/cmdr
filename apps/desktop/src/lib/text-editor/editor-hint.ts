/**
 * The one-time hint after F4: whether to point at the setting that picks the
 * editor, and whether to spend the flag that keeps it one-time.
 *
 * Pure, so the whole table is testable without a launch; `openFileInEditor` does
 * the asking, the writing, and the toast. The table and its reasons:
 * `DETAILS.md` § The hint.
 */

import type { EditorOpenReport } from '$lib/ipc/bindings'
import { SYSTEM_DEFAULT_EDITOR_CHOICE } from './text-editor-choice'

/** What the decision reads. */
export interface EditorHintInput {
  /** The stored `behavior.textEditorHintSeen` flag. */
  hintSeen: boolean
  /** The stored `behavior.textEditorApp` value this press launched with. */
  storedChoice: string
  /** What the launch answered, or `null` when it threw. */
  report: EditorOpenReport | null
}

/** What the caller then does. */
export interface EditorHint {
  /** Raise the hint toast. */
  showHint: boolean
  /** Spend the one-time flag, so the hint never comes back. */
  markSeen: boolean
}

const QUIET: EditorHint = { showHint: false, markSeen: false }

/**
 * Decide what this press says about the setting.
 *
 * - The hint is spent, or the launch threw: say nothing, write nothing.
 * - The stored choice isn't the system default: say nothing, and spend the flag.
 *   They already found the setting.
 * - Nobody answered whether other editors exist (Linux, or not asked): say
 *   nothing, write nothing.
 * - The system default is the only editor: say nothing, and ❗ leave the flag
 *   UNSPENT. Someone who installs Sublime Text next month is still owed the hint.
 * - Another editor is installed: show the hint and spend the flag.
 */
export function decideEditorHint({ hintSeen, storedChoice, report }: EditorHintInput): EditorHint {
  if (hintSeen || report === null) return QUIET
  if (storedChoice !== SYSTEM_DEFAULT_EDITOR_CHOICE) return { showHint: false, markSeen: true }
  if (report.otherEditorsInstalled !== true) return QUIET
  return { showHint: true, markSeen: true }
}
