/**
 * A tool group's header: one line saying what its calls looked at, counted per kind of thing
 * ("Looked around: 6 folders, 2 searches"), so a collapsed group of 12 calls still says what
 * happened. Every word is ours; the counts come from each line's typed subject.
 */

import { tString } from '#lib/intl/messages.svelte.js'
import { formatUnitList } from '#lib/intl/list-format.js'
import { formatInteger } from '#lib/intl/number-format.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { ToolSubject } from '#lib/ipc/bindings.js'
import type { RailToolCall } from './ask-cmdr-messages'

/** The kinds of thing a header counts, in the order it names them. */
const CATEGORIES = ['folder', 'file', 'search', 'check', 'plan', 'note', 'other'] as const
type GroupCategory = (typeof CATEGORIES)[number]

/** Which kind of thing each subject counts as. A `Record` over every kind, so a new subject
 * variant is a compile error here rather than a header that silently skips it. */
export const SUBJECT_CATEGORY: Record<ToolSubject['kind'], GroupCategory> = {
  panes: 'check',
  nothing: 'other',
  pane: 'other',
  setting: 'other',
  focusedPane: 'folder',
  folder: 'folder',
  place: 'folder',
  files: 'file',
  search: 'search',
  photoSearch: 'search',
  importantFolders: 'check',
  drives: 'check',
  volume: 'other',
  operationLog: 'check',
  operation: 'check',
  renamePlan: 'plan',
  suggestions: 'check',
  suggestionGroup: 'check',
  suggestionSweep: 'plan',
  serverSuggestion: 'plan',
  nothingToSuggest: 'other',
  memoryFile: 'note',
  unknownTool: 'other',
  unparsed: 'other',
}

/** Each category's counted words. Literal keys, so every one has a static call site. */
export const GROUP_CATEGORY_KEYS: Record<GroupCategory, MessageKey> = {
  folder: 'askCmdr.toolGroup.folders',
  file: 'askCmdr.toolGroup.files',
  search: 'askCmdr.toolGroup.searches',
  check: 'askCmdr.toolGroup.checks',
  plan: 'askCmdr.toolGroup.plans',
  note: 'askCmdr.toolGroup.notes',
  other: 'askCmdr.toolGroup.other',
}

/** How much one call adds to its category: every file a call asked about counts, so "3 files"
 * stays true when one call read three. A call with no subject yet counts once, as "other". */
function weight(subject: ToolSubject | null): { category: GroupCategory; count: number } {
  if (subject === null) return { category: 'other', count: 1 }
  const count = subject.kind === 'files' ? Math.max(subject.count, 1) : 1
  return { category: SUBJECT_CATEGORY[subject.kind], count }
}

/** The header of a group of `calls`: still looking while any runs, looked around once all settled. */
export function groupSummaryText(calls: RailToolCall[]): string {
  const counts = new Map<GroupCategory, number>()
  for (const call of calls) {
    const { category, count } = weight(call.subject)
    counts.set(category, (counts.get(category) ?? 0) + count)
  }
  const parts = CATEGORIES.flatMap((category) => {
    const count = counts.get(category)
    return count === undefined
      ? []
      : [tString(GROUP_CATEGORY_KEYS[category], { count, countText: formatInteger(count) })]
  })
  const running = calls.some((call) => call.running)
  return tString(running ? 'askCmdr.toolGroup.summaryRunning' : 'askCmdr.toolGroup.summary', {
    parts: formatUnitList(parts),
  })
}
