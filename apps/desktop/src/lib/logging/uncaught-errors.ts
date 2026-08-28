/**
 * Forwards uncaught frontend errors into the app log, so a crash in the webview
 * leaves a trace instead of dying in a console nobody reads.
 *
 * Without this, an uncaught throw or an unhandled promise rejection is invisible
 * everywhere that matters: no line in the Rust log, nothing in an error-report
 * bundle, nothing in a CI E2E run's output. A two-hour CI wedge where the panes
 * stopped listing while the rest of the app stayed healthy produced zero frontend
 * evidence for exactly this reason.
 *
 * Registered once from `routes/+layout.ts` (a stable module, like
 * `hmr-recovery.ts`) so the listeners survive layout re-evaluation during HMR.
 * `log.error` is forwarded to Rust in production too — the prod sinks carry
 * `error+` — so these land in the log file and in error-report bundles.
 */

import { getAppLogger } from './logger'

const log = getAppLogger('uncaught')

/** Renders a thrown value for the log: an `Error` keeps its stack, anything else stringifies. */
function describe(value: unknown): string {
  if (value instanceof Error) {
    return value.stack ?? `${value.name}: ${value.message}`
  }
  try {
    return typeof value === 'string' ? value : JSON.stringify(value)
  } catch {
    // A value that won't stringify (a cycle, a Proxy that throws) still deserves a line.
    return String(value)
  }
}

let registered = false

/**
 * Installs the `error` / `unhandledrejection` listeners. Idempotent, so an HMR
 * re-import can't stack duplicate handlers that log every failure twice.
 *
 * Neither listener calls `preventDefault`: the goal is to OBSERVE, never to
 * swallow. Swallowing would hide the failure from the browser's own reporting and
 * from `hmr-recovery`, which needs to see the rejection it recovers from.
 */
export function registerUncaughtErrorLogging(): void {
  if (registered || typeof window === 'undefined') return
  registered = true

  window.addEventListener('error', (event: ErrorEvent) => {
    // A failed resource load (`<img>`, `<script>`) also fires `error`, carrying
    // neither a thrown value nor a message. Those aren't crashes. The absent value
    // is `null` in some engines and `undefined` in others, so test for both.
    const thrown: unknown = event.error ?? null
    if (thrown === null && !event.message) return
    const { filename, lineno, colno } = event
    log.error('Uncaught error at {source}: {detail}', {
      source: filename ? `${filename}:${String(lineno)}:${String(colno)}` : 'unknown',
      detail: thrown !== null ? describe(thrown) : event.message,
    })
  })

  window.addEventListener('unhandledrejection', (event: PromiseRejectionEvent) => {
    log.error('Unhandled promise rejection: {detail}', { detail: describe(event.reason) })
  })
}
