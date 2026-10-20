// "Send feedback" command

import { commands } from '#lib/ipc/bindings.js'
import type { SendFeedbackResult } from '#lib/ipc/bindings.js'

export type { SendFeedbackResult }

/**
 * Sends a beta tester's feedback text (plus an optional reply-to email when they ticked the
 * attach-email box, and the past five hours of logs when they ticked that one) to the api-server,
 * which stores it and files it on the reports board.
 *
 * Returns a typed result the caller branches on (`result.kind`), never a message string. On any
 * unexpected throw (which shouldn't happen, the command catches its own errors) we degrade to
 * `softFailure` so the UI always has a result to show.
 */
export async function sendFeedback(
  feedbackText: string,
  email: string | undefined,
  includeLogs: boolean,
): Promise<SendFeedbackResult> {
  try {
    return await commands.sendFeedback(feedbackText, email ?? null, includeLogs)
  } catch {
    return { kind: 'softFailure' }
  }
}
