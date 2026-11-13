/**
 * The open "Tell me about this" card: one at a time, fed by its request's channel.
 *
 * `openAboutThis` starts a request; each event folds in through `applyAboutEvent`, which drops anything that isn't
 * this card's. `closeAboutThis` cancels the request (❌ never by dropping the channel: the backend can't see that) and
 * takes the card down. Opening on another item while a card is up closes the first one first, so its late events land
 * nowhere.
 *
 * ❗ The card shows on its first frame after the grace, the way the context menu does: once the plan and the verdict's
 * answer (planned or not, which the backend sends right after the plan) are in and `GRACE_MS` has passed (or the facts
 * settled, or the card is unavailable), so the rows that answer at once are already there and the dialog opens at its
 * size, with the verdict's place reserved or not from the first frame. `SHOW_ANYWAY_MS` caps the wait for a plan
 * that's slow to come.
 */

import { tick } from 'svelte'
import type { AboutAction } from '#lib/ipc/bindings.js'
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

/**
 * What the card's rows and actions do, which only the opener knows (the pane, its explorer). Both run AFTER the card
 * closes, so focus lands where the action went.
 */
export interface AboutHooks {
  /** Takes the pane to a place a row names (a folder's child, an app's Library folder). */
  goTo: (target: AboutGoTarget) => void
  /** Does one of the footer's actions for the card's subject. */
  act: (action: AboutAction, subject: AboutSubject) => void
}

let running: AboutRequestHandle | null = null
/** The open card's hooks: the opener's, since only it knows the pane. */
let hooks: AboutHooks | null = null
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
  const placed = card.plan !== null && card.verdict !== null
  if (card.unavailable !== null || card.settled || (graceOver && placed)) aboutThisState.shown = true
}

/** Opens the card for `subject`, closing any card already up. `openerHooks` do what its rows and actions ask. */
export function openAboutThis(subject: AboutSubject, openerHooks: AboutHooks | null = null): void {
  closeAboutThis()
  hooks = openerHooks
  const handle = startAbout(subject, (event) => {
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
  const opener = hooks
  closeAboutThis()
  opener?.goTo(target)
}

/**
 * Closes the card, then does `action` for its subject (a footer button or a "More" row). Closing first is what puts
 * focus where the action went: the pane, the delete dialog, Finder, the browser.
 *
 * ❗ The action waits for the card to be GONE (`tick()`), not only closed in state: until its dialog unmounts it's
 * still in the open-dialogs inventory, and the delete flow refuses to start behind an open dialog ("Something else is
 * open here").
 */
export function actFromCard(action: AboutAction): void {
  const opener = hooks
  const subject = aboutThisState.card?.subject
  closeAboutThis()
  if (!subject || !opener) return
  void tick().then(() => {
    opener.act(action, subject)
  })
}

/**
 * Stops the card's verdict alone (its Stop button): the card stays open, and the text so far stands once the backend
 * answers `stopped`. A no-op with no card up or no verdict in flight.
 */
export function stopAboutVerdict(): void {
  if (running) void running.stopVerdict()
}

/** Closes the card and stops its gathering and its verdict. A no-op with no card up. */
export function closeAboutThis(): void {
  if (running) void running.cancel()
  running = null
  hooks = null
  clearTimers()
  graceOver = false
  aboutThisState.card = null
  aboutThisState.shown = false
}
