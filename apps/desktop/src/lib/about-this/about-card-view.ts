/**
 * What the card draws, derived from its data: the sections in the plan's order, each row's answer or `pending`, and
 * which sections collapse.
 *
 * Pure. ❌ No rule about what's worth saying lives here: prominence comes typed inside each `unknown`, and the rows
 * come from the plan. The one layout decision is the collapse, which reads only that typed prominence.
 */

import type { AboutFact, FactKind, Identity, Section, UnknownWhy } from '#lib/ipc/bindings.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { AboutCardData } from './about-card'
import { UNKNOWN_WHY, absenceKey } from './about-words'

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
  /**
   * The one reason every row of a collapsed section shares, when it isn't "no record" (a folder the index hasn't
   * reached: "Not indexed yet"), so the line says that rather than "Nothing on record". `null`: the generic line.
   */
  collapsedWhy: UnknownWhy | null
}

/** The reason every row shares, when all of them are unknowns for the same one other than `noRecord`. */
function sharedWhy(rows: RowView[]): UnknownWhy | null {
  const whys = new Set(rows.map((row) => (row.fact?.fact.state === 'unknown' ? row.fact.fact.value.why : null)))
  const [only] = [...whys]
  return whys.size === 1 && only !== null && only !== 'noRecord' ? only : null
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
    const collapsed = card.settled && rows.length > 0 && rows.every((row) => isQuietAbsence(row.fact))
    views.push({ section: planned.section, rows, collapsed, collapsedWhy: collapsed ? sharedWhy(rows) : null })
  }
  return views
}

/**
 * The one muted line a collapsed section shows. A section of one row says that row's own absence ("Cmdr hasn't scored
 * how much this folder matters yet"), since the line stands in for the row and its label; several rows sharing a reason
 * say the reason; anything else is "Nothing on record".
 */
export function collapsedLine(view: Pick<SectionView, 'rows' | 'collapsedWhy'>): MessageKey {
  if (!view.collapsedWhy) return 'aboutThis.section.nothingOnRecord'
  const [only] = view.rows
  return view.rows.length === 1 ? absenceKey(only.kind, view.collapsedWhy) : UNKNOWN_WHY[view.collapsedWhy]
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

/** Below this, a size on disk that differs from the size is block rounding, not news. */
const ON_DISK_MIN_DIFFERENCE = 1_000_000

/**
 * Whether the size line adds what the item takes on disk: only when it differs enough to mean something (a sparse or
 * compressed file, blocks shared with a clone), never for the few KB of block rounding every small file has.
 */
export function showsOnDisk(identity: Pick<Identity, 'logicalSize' | 'physicalSize'>): boolean {
  const { logicalSize, physicalSize } = identity
  if (logicalSize === null || physicalSize === null) return false
  const difference = Math.abs(physicalSize - logicalSize)
  return difference >= ON_DISK_MIN_DIFFERENCE && difference >= logicalSize / 10
}
