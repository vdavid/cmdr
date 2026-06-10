// "Send feedback" command

import { commands } from '$lib/ipc/bindings'
import type { SendFeedbackResult } from '$lib/ipc/bindings'

export type { SendFeedbackResult }

/**
 * Sends a beta tester's feedback text (plus an optional reply-to email when they ticked the
 * attach-email box) to the api-server, which stores it and pings the maintainer's Discord.
 *
 * Returns a typed result the caller branches on (`result.kind`), never a message string. On any
 * unexpected throw (which shouldn't happen, the command catches its own errors) we degrade to
 * `softFailure` so the UI always has a result to show.
 */
export async function sendFeedback(feedbackText: string, email?: string): Promise<SendFeedbackResult> {
  try {
    return await commands.sendFeedback(feedbackText, email ?? null)
  } catch {
    return { kind: 'softFailure' }
  }
}
