/**
 * The three ways out of the Dock offer, and what each one records.
 *
 * Separate from `dock-nudge.ts` so the toast body can reach the answers without
 * importing the module that mounts it (`import-cycles`). Every exit lands on a
 * `dock_pin_answered`, and the toast frame's × reports `dismissed` rather than
 * `no`: "read it and declined" and "swept it away" are different answers, and
 * the split is the interesting half of the question.
 */

import { addToast, dismissToast } from '$lib/ui/toast'
import { addCmdrToDock, trackEvent } from '$lib/tauri-commands'
import { tString } from '$lib/intl/messages.svelte'
import { getAppLogger } from '$lib/logging/logger'
import type { DockPinFailure } from '$lib/ipc/bindings'
import type { MessageKey } from '$lib/intl/keys.gen'

const log = getAppLogger('dock')

/** How the offer ended. `dismissed` is the toast frame's ×, ❌ never an active refusal. */
export type DockPinAnswer = 'yes' | 'no' | 'dismissed'

/** Records how the offer ended. The one place `dock_pin_answered` is sent. */
export function recordDockPinAnswer(answer: DockPinAnswer): void {
  void trackEvent('dock_pin_answered', { answer })
}

/** "No, thanks": retire the toast and never ask again. */
export function declineDockPin(toastId: string): void {
  dismissToast(toastId)
  recordDockPinAnswer('no')
}

/**
 * "Yes, add it to my Dock": write the tile, then say as little as possible.
 *
 * The toast goes first because the Dock restart blinks the whole Dock for about
 * a second, and leaving the question on screen through that reads as if nothing
 * happened. The answer records the PRESS, ❌ never the outcome: what's being
 * measured is whether people want this, and `dock_pin_failed` carries the rest.
 */
export async function acceptDockPin(toastId: string): Promise<void> {
  dismissToast(toastId)
  recordDockPinAnswer('yes')

  const failure = await addCmdrToDock()
  if (!failure) {
    addToast(tString('main.dockPinNudge.added'), { level: 'success' })
    return
  }

  log.warn(`Cmdr didn't get into the Dock: ${failureReason(failure)}`)
  void trackEvent('dock_pin_failed', { reason: failureReason(failure) })
  addToast(tString(failureMessageKey(failure)), { level: 'warn' })
}

/**
 * The typed token a refusal reports: the blocker for a `blocked`, the variant
 * otherwise. Both are Rust enum names, ❌ never a message string, so a new
 * variant reaches the dashboard under its own name.
 */
function failureReason(failure: DockPinFailure): string {
  return failure.kind === 'blocked' ? failure.reason : failure.kind
}

/**
 * What to tell the user about a refusal.
 *
 * Two cases earn their own line. `dockNotRestarted` means the tile IS stored,
 * so saying the pin didn't happen would be a lie; `managedDock` is somebody
 * else's policy, worth naming because nothing the user does here will change
 * it. Everything else is one honest "it didn't take" plus the manual way in.
 */
function failureMessageKey(failure: DockPinFailure): MessageKey {
  if (failure.kind === 'dockNotRestarted') return 'main.dockPinNudge.addedButDockDidNotRestart'
  if (failure.kind === 'blocked' && failure.reason === 'managedDock') return 'main.dockPinNudge.managedDock'
  return 'main.dockPinNudge.notAdded'
}
