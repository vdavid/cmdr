/**
 * Answers one IPC command from the test, for the capture's surfaces whose trigger is a backend
 * state no fixture can produce: a server that wants an account, an S3 price, macOS's stored
 * Automation refusal. Everything the frontend does with the answer is the real code.
 *
 * Tauri's `__TAURI_INTERNALS__.invoke` and the object holding it are both non-replaceable from a
 * webview, so the stub lives in the `playwright-e2e` backend (`set_test_ipc_answer`).
 *
 * ❗ Always restore from a `finally`: a stage-only run shares its app with the shard's other specs,
 * and a stub left behind would answer theirs.
 */

import type { TauriPage } from '@srsholmes/tauri-playwright'

/** What the command answers: `ok` resolves the call, `err` rejects it with that value. */
export type StubbedAnswer = { ok: unknown } | { err: unknown }

/** Makes `command` answer `answer` until the returned function runs. Several may be up at once. */
export async function stubIpcAnswer(
  page: TauriPage,
  command: string,
  answer: StubbedAnswer,
): Promise<() => Promise<void>> {
  const set = (args: unknown) =>
    page.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_ipc_answer', ${JSON.stringify(args)})`)
  const isError = 'err' in answer
  await set({ command, answer: isError ? answer.err : answer.ok, isError })
  return async () => {
    await set({ command, answer: null, isError: false }).catch(() => {})
  }
}
