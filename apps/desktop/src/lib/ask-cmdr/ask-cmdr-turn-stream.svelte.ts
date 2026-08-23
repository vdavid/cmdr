/**
 * The main window's subscription to Ask Cmdr turn events, and the fan-out to the two things
 * that care.
 *
 * One listener, not two: every turn — a rail send or a wake nobody asked for — arrives on the
 * same conversation-keyed event, so the thread on screen and the session list read the same
 * stream and can't disagree about what happened.
 *
 * ⚠️ **Main window only.** The event reaches every window (settings, viewer), and none of them
 * hosts a rail. Starting the watch from `routes/(main)/+page.svelte` is what scopes it, the way
 * the operation-failure watch is scoped.
 *
 * It also lives here rather than inside either slice because it is the one place allowed to
 * know both: the sessions slice calls into the rail's trigger and the trigger never imports it
 * back, so a fan-out inside either would close that loop.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import { onAskCmdrTurn, type AskCmdrTurn } from '$lib/tauri-commands'
import { noteThreadDiscarded, noteThreadStarted } from './ask-cmdr-sessions.svelte'
import { handleTurnEvent } from './ask-cmdr-stream.svelte'

let unlisten: UnlistenFn | null = null

/** Subscribe the main window to every turn's progress. Call once at app init. */
export async function startAskCmdrTurnStream(): Promise<void> {
  if (unlisten) return
  unlisten = await onAskCmdrTurn(routeTurnEvent)
}

export function stopAskCmdrTurnStream(): void {
  if (!unlisten) return
  unlisten()
  unlisten = null
}

/** Hand one event to the thread view (which keeps it only if it's the thread on screen) and
 * to the session list (which reacts to a thread appearing or going away). Exported for the
 * unit tests, which drive the fan-out without a Tauri event bus. */
export function routeTurnEvent(turn: AskCmdrTurn): void {
  handleTurnEvent(turn)
  if (turn.event.type === 'started') noteThreadStarted(turn.conversationId)
  else if (turn.event.type === 'discarded') noteThreadDiscarded(turn.conversationId)
}
