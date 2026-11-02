/**
 * The question a tool is waiting on (`tools/ask.rs`), for a person to answer.
 *
 * ❌ Never stacks: with another modal up the backend hears who's in the way and refuses, typed. An
 * answer always reaches the backend, so a tool never waits out the expiry behind a question that
 * came down some other way: Escape, the ×, `dialog close`, and an error boundary all answer.
 */

import type { ToolAskRequested, ToolAskResponse } from '#lib/ipc/bindings.js'
import { answerToolAsk, onToolAskDismissed, onToolAskRequested } from '#lib/tauri-commands/index.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { blockingSoftDialogBesides } from '#lib/ui/open-dialogs.svelte.js'

const log = getAppLogger('toolAsk')

let question = $state<ToolAskRequested | null>(null)
let unlisten: (() => void)[] = []

export function getToolAskQuestion(): ToolAskRequested | null {
  return question
}

function send(requestId: string, response: ToolAskResponse): void {
  answerToolAsk(requestId, response).catch((error: unknown) => {
    log.warn("Couldn't send the answer to a question: {error}", { error })
  })
}

function answer(response: ToolAskResponse): void {
  if (question === null) return
  const { requestId } = question
  question = null
  send(requestId, response)
}

export function allowToolAsk(): void {
  answer({ kind: 'allowed' })
}

/** The no: the button, Escape, the ×, and `dialog close`. */
export function declineToolAsk(): void {
  answer({ kind: 'declined' })
}

/** The dialog broke or is going away, so nobody answered: fails closed with no decline on record. */
export function abandonToolAsk(): void {
  answer({ kind: 'gone' })
}

/** A question arrived. The backend read the dialogs a moment ago, so one may have opened since. */
export function receiveToolAsk(request: ToolAskRequested): void {
  const inTheWay = blockingSoftDialogBesides([])
  if (inTheWay !== null) {
    send(request.requestId, { kind: 'blocked', dialog: inTheWay })
    return
  }
  question = request
}

/** The tool stopped waiting: Stop, a client gone, the expiry, or its own answer coming back. */
export function receiveToolAskDismissed(requestId: string): void {
  if (question?.requestId === requestId) question = null
}

export async function setupToolAsk(): Promise<void> {
  unlisten = [
    await onToolAskRequested(receiveToolAsk),
    await onToolAskDismissed(({ requestId }) => {
      receiveToolAskDismissed(requestId)
    }),
  ]
}

export function cleanupToolAsk(): void {
  abandonToolAsk()
  for (const stop of unlisten) stop()
  unlisten = []
}
