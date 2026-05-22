/**
 * Pure helper for the dialog's `⏎` ownership swap (R3 D8).
 *
 * `lastDialogEvent` × `resultsCount > 0` determines whether bare Enter runs the
 * query or opens the cursor row. See `lib/query-ui/CLAUDE.md` § Keyboard contract.
 *
 * Lives in its own file so consumers (Search dialog, Selection dialog, and any
 * future QueryDialog primitive) can import the helper without pulling the whole
 * factory.
 */

export type LastDialogEvent = 'opened' | 'results-arrived' | 'cursor-moved' | 'query-edited' | 'filter-edited'

export type EnterAction = 'go-to-file' | 'run-search'

export function deriveEnterAction(input: { lastEvent: LastDialogEvent; resultsCount: number }): EnterAction {
  if (input.resultsCount <= 0) return 'run-search'
  if (input.lastEvent === 'results-arrived' || input.lastEvent === 'cursor-moved') {
    return 'go-to-file'
  }
  return 'run-search'
}
