// Beta-tester signup commands

import { commands } from '$lib/ipc/bindings'
import type { BetaSignupResult } from '$lib/ipc/bindings'

export type { BetaSignupResult }

/**
 * Subscribes a beta contact email to the mailing list. The backend sends ONLY the email (never an
 * install id), so the email and the anonymous usage data never co-occur on our servers.
 *
 * Returns a typed result the caller branches on (`result.kind`), never a message string. On any
 * unexpected throw (which shouldn't happen, the command catches its own errors) we degrade to
 * `softFailure` so the UI always has a result to show.
 */
export async function betaSignup(email: string): Promise<BetaSignupResult> {
  try {
    return await commands.betaSignup(email)
  } catch {
    return { kind: 'softFailure' }
  }
}
