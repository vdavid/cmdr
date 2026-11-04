/**
 * The record of commands the demo has no answer for, exposed as `window.__cmdrDemo.unanswered`
 * (name → call count) so a probe or a curious developer sees what a session asked for in vain.
 */
const unanswered = new Map<string, number>()

/** Counts one unanswered call, logs the first per command, and answers `null`: right for a void command. */
export function noteUnanswered(command: string, payload: unknown): null {
  const count = (unanswered.get(command) ?? 0) + 1
  unanswered.set(command, count)
  // The demo has no backend log to write to: the devtools console is where its developer looks.
  // eslint-disable-next-line no-console -- see above
  if (count === 1) console.debug('[demo] no answer for', command, payload)
  return null
}

export function exposeUnanswered(): void {
  ;(window as unknown as { __cmdrDemo: unknown }).__cmdrDemo = { unanswered }
}
