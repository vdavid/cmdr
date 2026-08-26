/**
 * What the status corner shows about the proactive agent: a wake thinking right now, or the gate
 * standing between the agent and being able to notice anything.
 *
 * **Subscribed, never polled.** `agent-wake-status` carries both facts, so being told is the only
 * thing that moves this. One read at startup seeds it, because a wake already running when the
 * window opened announced itself before anyone was listening, and a gate that closed before then
 * did too.
 *
 * ⚠️ **The subscription lives here, not in the component.** `StatusCorner` is mounted by two test
 * suites that stub neither Tauri nor this module, so a member opening a listener at mount would
 * break both. The component reads `$state` and nothing else, exactly as the suggestions badge does.
 *
 * ⚠️ **Main window only.** The event reaches every window and only this one has a status corner,
 * so `routes/(main)/window-services.ts` is what scopes it.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import { getAppLogger } from '$lib/logging/logger'
import { getSetting, onSpecificSettingChange } from '$lib/settings'
import { agentWakeStatus, cancelAskCmdr, onAgentWakeStatus, type WakeReadinessView } from '$lib/tauri-commands'
import { openRail, switchToThread } from './ask-cmdr-trigger.svelte'

const log = getAppLogger('askCmdr')

interface WakeIndicatorState {
  /** The thread a wake is writing into right now, `null` when none is. Also the click target. */
  thinkingIn: number | null
  /** Which of the three gates is in the way, `'ready'` when none is. */
  readiness: WakeReadinessView
  /** Whether the user asked the agent to watch at all. The corner is silent when they didn't. */
  proactive: boolean
}

export const wakeIndicator = $state<WakeIndicatorState>({
  thinkingIn: null,
  readiness: 'needsConsent',
  proactive: false,
})

/**
 * What the corner should render, as one token.
 *
 * ⚠️ **`'silent'` covers two cases on purpose**, and it is the resolution of a contradiction the
 * two halves of this feature used to state differently. `readiness.rs` says every gap is worth
 * reporting, because a user who declined disk access and a user with a tidy Downloads folder
 * otherwise see the identical nothing. `SuggestedOpsIndicator` says a control for a feature with
 * nothing to say is noise. Both are right, about different users: the gap is for somebody who
 * opted IN and hit a wall. Somebody who never consented, or who turned the proactive loop off,
 * gets nothing — an always-present AI nag is exactly what they said no to.
 *
 * A running wake shows REGARDLESS of the setting: it is spending the user's money right now and
 * has to be visible and stoppable, however it was started (a forced wake, or a setting turned off
 * mid-turn).
 */
export type WakeIndicatorMode = 'silent' | 'thinking' | 'needsFullDiskAccess' | 'needsApiKey'

export function wakeIndicatorMode(state: WakeIndicatorState): WakeIndicatorMode {
  if (state.thinkingIn !== null) return 'thinking'
  if (!state.proactive || state.readiness === 'needsConsent' || state.readiness === 'ready') return 'silent'
  return state.readiness
}

let unlisten: UnlistenFn | null = null
let unsubscribeSetting: (() => void) | null = null

/** Seed the indicator and subscribe. Call once at app init, from the main window only. */
export async function startWakeIndicator(): Promise<void> {
  if (unlisten) return
  wakeIndicator.proactive = getSetting('askCmdr.proactive')
  unsubscribeSetting = onSpecificSettingChange('askCmdr.proactive', (value) => {
    wakeIndicator.proactive = value
  })
  unlisten = await onAgentWakeStatus((status) => {
    wakeIndicator.thinkingIn = status.phase.phase === 'thinking' ? status.phase.conversationId : null
    wakeIndicator.readiness = status.readiness
  })
  await seedFromBackend()
}

/** Read the current status once, for the moves no event will announce to this window. */
async function seedFromBackend(): Promise<void> {
  try {
    const status = await agentWakeStatus()
    wakeIndicator.thinkingIn = status.phase.phase === 'thinking' ? status.phase.conversationId : null
    wakeIndicator.readiness = status.readiness
  } catch (e) {
    // An indicator that can't be seeded stays silent rather than guessing. The next event
    // corrects it, and nothing the user does depends on this being right to the second.
    log.warn("Couldn't seed the wake indicator: {error}", { error: String(e) })
  }
}

export function stopWakeIndicator(): void {
  unsubscribeSetting?.()
  unsubscribeSetting = null
  if (!unlisten) return
  unlisten()
  unlisten = null
}

/**
 * Open the rail on the thread the running wake is writing into.
 *
 * ⚠️ `switchToThread` BEFORE `openRail`: a closed→open transition otherwise bootstraps the most
 * recent thread and wastes a fetch on a thread we're about to replace. The turn's own events keep
 * arriving on the conversation-keyed stream, so the rail fills in as the wake writes.
 */
export async function openWakeThread(): Promise<void> {
  const id = wakeIndicator.thinkingIn
  if (id === null) return
  await switchToThread(id)
  await openRail()
}

/**
 * Stop the running wake.
 *
 * The same command the rail's Stop uses: a wake registers its cancel token in the one registry
 * (`agent/chat/cancel.rs`), so there is no wake-specific stop to keep in step with this one.
 */
export async function stopWake(): Promise<void> {
  const id = wakeIndicator.thinkingIn
  if (id === null) return
  await cancelAskCmdr(id)
}
