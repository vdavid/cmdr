/**
 * Test helpers for IPC contract tests.
 *
 * Wraps `@tauri-apps/api/mocks::mockIPC` with two affordances we want at the call sites:
 *
 *  1. A captured call log (`recorder.calls`): every command name + payload pair the bindings
 *     send, in order. Lets tests assert on the **snake_case command name** (the contract that
 *     drift can break) and the **camelCase payload shape** the bindings build.
 *  2. A typed responder map: `mock(commandName, handler)` registers a per-command response
 *     or a thrown error. Unmatched commands surface as a clear test failure instead of a
 *     mysterious `undefined`.
 *
 * **What this layer catches**: argument coercion, serde-shape drift between the FE binding
 * call site and the Rust command signature, and that side-effecting commands hit IPC at all.
 *
 * **What this layer doesn't catch**: real permission gating (the Tauri permission system is
 * skipped (`mockIPC` patches `__TAURI_INTERNALS__.invoke` before it gets there), business
 * logic in `*_core` helpers (Rust unit tests own that), or end-to-end UI behaviour
 * (Playwright owns that).
 *
 * See `apps/desktop/src/lib/ipc/CLAUDE.md` for the broader IPC architecture.
 */

import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import type { InvokeArgs } from '@tauri-apps/api/core'

export type IpcCall = {
  command: string
  payload: InvokeArgs | undefined
}

export type IpcResponder = (payload: InvokeArgs | undefined) => unknown

/** Recorder returned by `installIpcMock`. */
export type IpcRecorder = {
  /** All IPC calls captured since this recorder was installed, in invocation order. */
  readonly calls: ReadonlyArray<IpcCall>
  /**
   * Register a responder for a snake_case command. If the responder throws (or returns a
   * rejected promise), `commands.foo(...)` resolves to `{ status: 'error', error }` via
   * the `typedError` runtime in `bindings.ts`. Plain values resolve to the data branch.
   */
  mock: (command: string, responder: IpcResponder) => void
  /** Convenience: find the most recent call for a given command, or `undefined`. */
  lastCall: (command: string) => IpcCall | undefined
  /** Convenience: count of calls for a given command. */
  callCount: (command: string) => number
}

/**
 * Install the mock IPC layer for one test. Pair with `clearIpcMocks` in `afterEach`.
 *
 * The default fallback for unregistered commands is to throw a descriptive error so a
 * missed mock surfaces clearly rather than as a silent `undefined`.
 */
export function installIpcMock(): IpcRecorder {
  const calls: IpcCall[] = []
  const responders = new Map<string, IpcResponder>()

  const recorder: IpcRecorder = {
    calls,
    mock(command, responder) {
      responders.set(command, responder)
    },
    lastCall(command) {
      for (let i = calls.length - 1; i >= 0; i--) {
        if (calls[i].command === command) return calls[i]
      }
      return undefined
    },
    callCount(command) {
      return calls.reduce((n, call) => (call.command === command ? n + 1 : n), 0)
    },
  }

  mockIPC((cmd, payload) => {
    calls.push({ command: cmd, payload })
    const responder = responders.get(cmd)
    if (!responder) {
      throw new Error(
        `installIpcMock: unmocked command '${cmd}'. Register a responder with recorder.mock('${cmd}', …).`,
      )
    }
    return responder(payload)
  })

  return recorder
}

/** Tear down the mock layer. Call in `afterEach`. */
export function clearIpcMocks(): void {
  clearMocks()
}
