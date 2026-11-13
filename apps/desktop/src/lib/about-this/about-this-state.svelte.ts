/**
 * The open "Tell me about this" card: one at a time, fed by its request's channel.
 *
 * `openAboutThis` starts a request; each event folds in through `applyAboutEvent`, which drops anything that isn't
 * this card's. `closeAboutThis` cancels the request (❌ never by dropping the channel: the backend can't see that) and
 * takes the card down. Opening on another item while a card is up closes the first one first, so its late events land
 * nowhere.
 *
 * ❗ The card shows on its first frame after the grace, the way the context menu does: once the plan is in and
 * `GRACE_MS` has passed (or the facts settled, or the card is unavailable), so the rows that answer at once are
 * already there and the dialog opens at its size. `SHOW_ANYWAY_MS` caps the wait for a plan that's slow to come.
 */

import { startAbout, type AboutRequestHandle } from '#lib/tauri-commands/index.js'
import { applyAboutEvent, emptyCard, type AboutCardData, type AboutGoTarget, type AboutSubject } from './about-card'

/** The context menu's grace (`menu/context_menu_facts.rs` `GRACE`): what answers inside it is there on frame one. */
export const GRACE_MS = 100
/** Past this with no plan, the card shows what it has (its title and placeholders). */
export const SHOW_ANYWAY_MS = 1500

interface AboutThisState {
  /** The card being gathered, or `null`. */
  card: AboutCardData | null
  /** Whether the dialog is on screen yet (see the grace above). */
  shown: boolean
}

/** The card. Read it; change it only through `openAboutThis` / `closeAboutThis`. */
export const aboutThisState: AboutThisState = $state({ card: null, shown: false })

let running: AboutRequestHandle | null = null
/** Where the open card's rows take the pane: the opener's, since only it knows the pane. */
let goTo: ((target: AboutGoTarget) => void) | null = null
let graceOver = false
let timers: ReturnType<typeof setTimeout>[] = []

function clearTimers(): void {
  for (const timer of timers) clearTimeout(timer)
  timers = []
}

/** Shows the card once it has something worth a first frame. */
function showIfReady(): void {
  const card = aboutThisState.card
  if (!card || aboutThisState.shown) return
  if (card.unavailable !== null || card.settled || (graceOver && card.plan !== null)) aboutThisState.shown = true
}

/**
 * Opens the card for `subject`, closing any card already up. `onGoTo` takes the pane to a place a row names (a
 * folder's child, an app's Library folder).
 */
export function openAboutThis(subject: AboutSubject, onGoTo: ((target: AboutGoTarget) => void) | null = null): void {
  closeAboutThis()
  goTo = onGoTo
  const handle = startAbout(subject.path, subject.volumeId, (event) => {
    const card = aboutThisState.card
    if (!card) return
    const next = applyAboutEvent(card, event)
    if (next === card) return
    aboutThisState.card = next
    showIfReady()
  })
  running = handle
  aboutThisState.card = emptyCard(handle.requestId, subject)
  timers.push(
    setTimeout(() => {
      graceOver = true
      showIfReady()
    }, GRACE_MS),
    setTimeout(() => {
      if (aboutThisState.card) aboutThisState.shown = true
    }, SHOW_ANYWAY_MS),
  )
}

/** Closes the card, then takes the pane to `target` (Enter on a row that names a place). */
export function goToFromCard(target: AboutGoTarget): void {
  const go = goTo
  closeAboutThis()
  go?.(target)
}

/** Closes the card and stops its gathering. A no-op with no card up. */
export function closeAboutThis(): void {
  if (running) void running.cancel()
  running = null
  goTo = null
  clearTimers()
  graceOver = false
  aboutThisState.card = null
  aboutThisState.shown = false
}
