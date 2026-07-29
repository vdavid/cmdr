/**
 * What the dialog does with the current result set: bare ⏎, ⌥⏎, a row click, and the two
 * footer buttons.
 *
 * Every function here reads `config.state` and calls the consumer's action handlers; none
 * of them writes state, so they stay unit-testable against a plain config. The
 * Selection-style fallback is the rule worth remembering: a consumer with no
 * `secondaryAction` sends every "open the cursor row" gesture to the primary action over
 * the WHOLE set instead.
 */

import type { EnterAction } from './enter-action'
import type { QueryDialogConfig } from './query-dialog-config'

/** Footer primary button: hands the current result set to the primary action. */
export function activatePrimary<E>(config: QueryDialogConfig<E>): void {
  const results = config.state.getResults()
  if (config.primaryAction) void config.primaryAction.handler(results)
}

/**
 * ⌥⏎: same action, but only when there's something to act on. The footer button carries
 * its own `disabled` state instead, so the two paths guard differently on purpose.
 */
export function activatePrimaryOnResults<E>(config: QueryDialogConfig<E>): void {
  const results = config.state.getResults()
  if (results.length > 0 && config.primaryAction) void config.primaryAction.handler(results)
}

/** Footer secondary button: acts on the cursor row, or nothing when the cursor is off-list. */
export function activateSecondaryAtCursor<E>(config: QueryDialogConfig<E>): void {
  if (!config.secondaryAction) return
  const results = config.state.getResults()
  const index = config.state.getCursorIndex()
  if (index < 0 || index >= results.length) return
  void config.secondaryAction.handler(results[index])
}

/** Row click: open that row, or fall back to the primary action over the whole set. */
export function activateResultAt<E>(config: QueryDialogConfig<E>, index: number): void {
  const results = config.state.getResults()
  if (index >= results.length) return
  if (config.secondaryAction) {
    void config.secondaryAction.handler(results[index])
    return
  }
  if (config.primaryAction) void config.primaryAction.handler(results)
}

/**
 * Bare Enter per D8: dispatches on `enterAction`.
 *   - 'go-to-file': fires `secondaryAction.handler(currentEntry)`. If no secondary action
 *     exists (Selection), falls through to the primary action.
 *   - 'run-search': `run()` fires the active mode's query (AI / filename / regex).
 */
export function dispatchEnterAction<E>(
  config: QueryDialogConfig<E>,
  enterAction: EnterAction,
  run: () => void,
): void {
  if (enterAction !== 'go-to-file') {
    run()
    return
  }
  const results = config.state.getResults()
  if (config.secondaryAction) {
    const index = config.state.getCursorIndex()
    if (index >= 0 && index < results.length) void config.secondaryAction.handler(results[index])
    return
  }
  // Selection-style: no secondary; fall through to primary on the result set.
  if (config.primaryAction && results.length > 0) void config.primaryAction.handler(results)
}
