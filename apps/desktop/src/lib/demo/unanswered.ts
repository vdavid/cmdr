/**
 * The commands the demo has no answer for. A void one resolves (its caller expects nothing back); any
 * other rejects, as Tauri rejects a command it lacks, so a `null` never poses as a real answer (one
 * crashed the search dialog). Which are void comes from the generated bindings, at build time
 * (`scripts/vite-demo-void-commands.ts`). Exposed as `window.__cmdrDemo.unanswered` (name → call
 * count) so a probe or a curious developer sees what a session asked for in vain.
 */
import voidCommands from 'virtual:cmdr-demo-void-commands'

const unanswered = new Map<string, number>()

/**
 * Counts one unanswered call and logs the first per command. Answers `null` for a void command, and
 * throws (so the backend's promise rejects) for any other.
 */
export function noteUnanswered(command: string, payload: unknown): null {
  const count = (unanswered.get(command) ?? 0) + 1
  unanswered.set(command, count)
  // The demo has no backend log to write to: the devtools console is where its developer looks.
  // eslint-disable-next-line no-console -- see above
  if (count === 1) console.debug('[demo] no answer for', command, payload)
  if (voidCommands.has(command)) return null
  // Tauri rejects with a plain string, which the typed wrappers pass on as the error.
  // eslint-disable-next-line @typescript-eslint/only-throw-error -- the wire shape Tauri rejects with
  throw `the browser demo doesn't answer \`${command}\``
}

export function exposeUnanswered(): void {
  ;(window as unknown as { __cmdrDemo: unknown }).__cmdrDemo = { unanswered }
}
