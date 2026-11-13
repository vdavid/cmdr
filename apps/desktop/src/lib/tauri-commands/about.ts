// "Tell me about this": the card's stream and its cancel.

import { Channel, invoke } from '@tauri-apps/api/core'
import { commands, type AboutEvent } from '#lib/ipc/bindings.js'

/** One card's request: its id (minted here) and how to stop it. */
export interface AboutRequestHandle {
  /** The id every event of this request carries. */
  requestId: string
  /** Stops the gathering. Idempotent, and safe before `about_start` has resolved. */
  cancel: () => Promise<void>
}

/**
 * Starts gathering the card for `path`, calling `onEvent` with each event as it lands: `planned` first (or
 * `unavailable` alone), then one `fact` per row in any order, then `factsSettled`.
 *
 * ❗ Stop it with the returned `cancel`, ❌ never by dropping the channel: a Tauri `Channel`'s `send` succeeds after
 * the handler is gone, so the backend can't see an abandoned card. The id is minted here, so a cancel that races
 * ahead of the start still lands (the backend keeps it as a tombstone).
 */
export function startAbout(
  item: { path: string; volumeId: string | null; isDirectory: boolean },
  onEvent: (event: AboutEvent) => void,
): AboutRequestHandle {
  const { path, volumeId, isDirectory } = item
  const requestId = crypto.randomUUID()
  const channel = new Channel<AboutEvent>()
  channel.onmessage = onEvent
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- streaming Channel<T> not specta-friendly yet; tracked for follow-up
  invoke('about_start', { request: { requestId, path, volumeId, isDirectory }, onEvent: channel }).catch(() => {
    // The command returns nothing; a webview teardown can reject the promise, which changes nothing here.
  })
  const cancel = async (): Promise<void> => {
    try {
      await commands.aboutCancel(requestId)
    } catch {
      // Idempotent: the run may already be gone.
    }
  }
  return { requestId, cancel }
}
