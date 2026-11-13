/**
 * The card's data, and the one reducer that folds the backend's events into it.
 *
 * Pure: no runes, no IPC. The live slice (`about-this-state.svelte.ts`) and the gallery's fixtures both build a card
 * through `applyAboutEvent`, so a previewed card is exactly what that stream of events renders.
 *
 * ❗ Every rule about WHAT the card says (which rows exist, what's worth a sentence, the headline, the removal row)
 * comes typed from `cmdr-about`. This file only keeps the events in order and drops the ones that don't belong.
 */

import type { AboutEvent, AboutFact, AboutPlan, AboutUnavailable, Derived, FactKind } from '#lib/ipc/bindings.js'

/** What the card asks about, as the pane held it when the card opened. */
export interface AboutSubject {
  /** The item's path. */
  path: string
  /** The pane's volume, or `null` to let the backend place the bare path. */
  volumeId: string | null
  /** The pane's name for it, shown in the title from the first frame. */
  name: string
  /**
   * Whether the pane listed it as a folder. The backend plans an item it reaches through its volume (a share, a
   * phone, an archive, git's history) from this, so a share that stopped answering still gets its card at once.
   */
  isDirectory: boolean
  /**
   * The pane's icon id for it (`FileEntry.iconId`), so the title shows the icon the pane shows (an app's own icon for
   * an app). The pane already fetched it, so the card reads the icon cache and never fetches. `null`: a glyph.
   */
  iconId: string | null
}

/**
 * A place the card can take the pane: a folder it names (a child, an app's Library folder), or a file, shown in its
 * folder with the cursor on it. Opened by Enter on that row, which closes the card.
 */
export interface AboutGoTarget {
  path: string
  isFolder: boolean
}

/** One card, from its open to whatever has landed so far. */
export interface AboutCardData {
  /** The request every event for this card carries; anything else is dropped. */
  requestId: string
  subject: AboutSubject
  /** The rows the card has, in display order. `null` until `planned` arrives. */
  plan: AboutPlan | null
  /** Each row's answer as it landed. A planned row with no entry here is still pending. */
  facts: Partial<Record<FactKind, AboutFact>>
  /** The headline and the removal row as the backend last derived them. */
  derived: Derived | null
  /** Every planned row has its answer. */
  settled: boolean
  /** Why the card can't be gathered at all, instead of everything above. */
  unavailable: AboutUnavailable | null
}

/** A card that has asked and heard nothing yet. */
export function emptyCard(requestId: string, subject: AboutSubject): AboutCardData {
  return { requestId, subject, plan: null, facts: {}, derived: null, settled: false, unavailable: null }
}

/** Whether `kind` is one of the plan's rows. */
function isPlanned(plan: AboutPlan, kind: FactKind): boolean {
  return plan.sections.some((section) => section.kinds.includes(kind))
}

/** The volume stopped answering. The note's slot is the plan's own, there from the first frame: only its words change. */
function volumeStopped(card: AboutCardData): AboutCardData {
  const note = card.plan?.volumeNote
  if (card.plan === null || card.settled || !note || note.limit === 'unreachable') return card
  return { ...card, plan: { ...card.plan, volumeNote: { ...note, limit: 'unreachable' } } }
}

/**
 * Folds one event into the card. Returns the same object when the event changes nothing, so a caller can skip a
 * render.
 *
 * Drops, on purpose: an event for another request (a card closed and reopened on another item must never show the
 * first item's facts), a fact before the plan or outside it, anything after `factsSettled` or `unavailable`, and a
 * second plan.
 */
export function applyAboutEvent(card: AboutCardData, event: AboutEvent): AboutCardData {
  if (event.requestId !== card.requestId || card.unavailable !== null) return card
  const update = event.update
  switch (update.type) {
    case 'planned':
      if (card.plan !== null) return card
      return { ...card, plan: update.plan, derived: update.derived }
    case 'fact': {
      if (card.plan === null || card.settled || !isPlanned(card.plan, update.fact.kind)) return card
      return {
        ...card,
        facts: { ...card.facts, [update.fact.kind]: update.fact },
        derived: update.derived,
      }
    }
    case 'volumeStoppedAnswering':
      return volumeStopped(card)
    case 'factsSettled':
      if (card.plan === null || card.settled) return card
      return { ...card, settled: true }
    case 'unavailable':
      if (card.plan !== null) return card
      return { ...card, unavailable: update.why, settled: true }
  }
}
