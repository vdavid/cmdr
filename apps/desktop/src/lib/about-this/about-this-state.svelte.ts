/**
 * The open "Tell me about this" card: one at a time, fed by its request's channel.
 *
 * `openAboutThis` starts a request and shows the card at once; each event folds in through `applyAboutEvent`, which
 * drops anything that isn't this card's. `closeAboutThis` cancels the request (❌ never by dropping the channel: the
 * backend can't see that) and takes the card down. Opening on another item while a card is up closes the first one
 * first, so its late events land nowhere.
 */

import { startAbout, type AboutRequestHandle } from '#lib/tauri-commands/index.js'
import { applyAboutEvent, emptyCard, type AboutCardData, type AboutSubject } from './about-card'

interface AboutThisState {
  /** The card on screen, or `null`. */
  card: AboutCardData | null
}

/** The card on screen. Read it; change it only through `openAboutThis` / `closeAboutThis`. */
export const aboutThisState: AboutThisState = $state({ card: null })

let running: AboutRequestHandle | null = null

/** Opens the card for `subject`, closing any card already up. */
export function openAboutThis(subject: AboutSubject): void {
  closeAboutThis()
  const handle = startAbout(subject.path, subject.volumeId, (event) => {
    const card = aboutThisState.card
    if (!card) return
    const next = applyAboutEvent(card, event)
    if (next !== card) aboutThisState.card = next
  })
  running = handle
  aboutThisState.card = emptyCard(handle.requestId, subject)
}

/** Closes the card and stops its gathering. A no-op with no card up. */
export function closeAboutThis(): void {
  if (running) void running.cancel()
  running = null
  aboutThisState.card = null
}
