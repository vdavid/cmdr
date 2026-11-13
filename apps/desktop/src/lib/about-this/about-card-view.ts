/**
 * What the card draws, derived from its data: the sections in the plan's order, each row's answer or `pending`, and
 * which sections collapse.
 *
 * Pure. ❌ No rule about what's worth saying lives here: prominence comes typed inside each `unknown`, and the rows
 * come from the plan. The one layout decision is the collapse, which reads only that typed prominence.
 */

import type { AboutFact, FactKind, Section } from '#lib/ipc/bindings.js'
import type { AboutCardData } from './about-card'

/** One planned row: its answer, or `null` while it's pending. */
export interface RowView {
  kind: FactKind
  fact: AboutFact | null
}

/** One section with its rows. */
export interface SectionView {
  section: Exclude<Section, 'removal'>
  rows: RowView[]
  /**
   * Drawn as its heading plus one muted line: every row settled as an ordinary absence. Only once the facts settle,
   * so the one layout change happens at the end, never mid-stream.
   */
  collapsed: boolean
}

/** Whether a row's answer is an ordinary absence: a Routine unknown, or a question with no answer here. */
function isQuietAbsence(fact: AboutFact | null): boolean {
  if (!fact) return false
  const state = fact.fact
  if (state.state === 'notApplicable') return true
  return state.state === 'unknown' && state.value.prominence === 'routine'
}

/** The evidence sections in display order. The removal section is drawn on its own, from `derived`. */
export function sectionsOf(card: AboutCardData): SectionView[] {
  if (!card.plan) return []
  const views: SectionView[] = []
  for (const planned of card.plan.sections) {
    if (planned.section === 'removal') continue
    const rows = planned.kinds.map((kind) => ({ kind, fact: card.facts[kind] ?? null }))
    views.push({
      section: planned.section,
      rows,
      collapsed: card.settled && rows.length > 0 && rows.every((row) => isQuietAbsence(row.fact)),
    })
  }
  return views
}

/** How a card's rows have landed so far: the calm-card budget a plain document must meet. */
export interface RowTally {
  known: number
  unknown: number
  pending: number
}

/** Counts the planned rows by state. `notApplicable` counts as unknown: it's still a row that says no. */
export function tallyRows(card: AboutCardData): RowTally {
  const tally: RowTally = { known: 0, unknown: 0, pending: 0 }
  for (const section of sectionsOf(card)) {
    for (const row of section.rows) {
      if (!row.fact || row.fact.fact.state === 'pending') tally.pending += 1
      else if (row.fact.fact.state === 'known') tally.known += 1
      else tally.unknown += 1
    }
  }
  return tally
}
