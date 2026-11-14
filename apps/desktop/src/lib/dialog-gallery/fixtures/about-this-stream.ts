/**
 * The pieces every `tell-me-about-this` fixture is built from: a recorded stream of the backend's events, folded
 * through the card's own reducer, so a preview is exactly what that stream renders live.
 */

import type {
  AboutEvent,
  Actions,
  AboutFact,
  AboutPlan,
  Derived,
  Fact,
  Headline,
  HiddenInName,
  PlannedSection,
  RemovalConsequence,
  UnknownWhy,
} from '#lib/ipc/bindings.js'
import { applyAboutEvent, emptyCard, type AboutCardData } from '#lib/about-this/about-card.js'

export const REQUEST = 'gallery'

/** Seconds since the epoch for a calendar day, at noon UTC. */
export function day(iso: string): number {
  return Math.floor(Date.parse(`${iso}T12:00:00Z`) / 1000)
}

export function routine(why: UnknownWhy): Fact<never> {
  return { state: 'unknown', value: { why, prominence: 'routine' } }
}
export function notable(why: UnknownWhy): Fact<never> {
  return { state: 'unknown', value: { why, prominence: 'notable' } }
}

/** No footer actions: what a card derives before any fact lands on a volume that allows none. */
export const NO_ACTIONS: Actions = { shown: [], more: [] }

export function derived(
  headline: Headline,
  shown: RemovalConsequence[],
  more: RemovalConsequence[] = [],
  actions: Actions = NO_ACTIONS,
): Derived {
  return { headline, removal: { shown, more }, actions }
}

export interface Stream {
  plan: AboutPlan
  /** The headline and removal as they stand with every row pending. */
  first: Derived
  /** Each landing, with what the headline and removal say after it. */
  facts: { fact: AboutFact; derived: Derived }[]
  /** The volume stopped answering right after the plan, before any row landed. */
  stopped?: boolean
  settled: boolean
}

/**
 * The card a stream renders. `after` are more updates folded in after it: the verdict's events, which follow the plan
 * and may run past the facts' settle.
 */
export function card(name: string, path: string, stream: Stream, after: AboutEvent['update'][] = []): AboutCardData {
  const events: AboutEvent[] = [
    { requestId: REQUEST, update: { type: 'planned', plan: stream.plan, derived: stream.first } },
    ...(stream.stopped
      ? [{ requestId: REQUEST, update: { type: 'volumeStoppedAnswering' } } satisfies AboutEvent]
      : []),
    ...stream.facts.map((landing): AboutEvent => ({
      requestId: REQUEST,
      update: { type: 'fact', fact: landing.fact, derived: landing.derived },
    })),
  ]
  if (stream.settled) events.push({ requestId: REQUEST, update: { type: 'factsSettled' } })
  for (const update of after) events.push({ requestId: REQUEST, update })
  return events.reduce(
    applyAboutEvent,
    emptyCard(REQUEST, { path, volumeId: 'root', name, iconId: null, isDirectory: false }),
  )
}

export function plan(
  target: AboutPlan['target'],
  sections: PlannedSection[],
  volumeNote: AboutPlan['volumeNote'] = null,
): AboutPlan {
  return { target, sections: [...sections, { section: 'removal', kinds: [] }], volumeNote }
}

export function identity(
  name: string,
  size: number | null,
  modified: string,
  extra: { hardLinks?: number; symlinkTarget?: string; nameHid?: HiddenInName } = {},
): AboutFact {
  return {
    kind: 'identity',
    fact: {
      state: 'known',
      value: {
        name,
        nameHid: extra.nameHid ?? null,
        logicalSize: size,
        physicalSize: size === null ? null : Math.ceil(size / 4096) * 4096,
        created: day(modified) - 86_400 * 3,
        modified: day(modified),
        hardLinks: extra.hardLinks ?? 1,
        symlinkTarget: extra.symlinkTarget ?? null,
      },
    },
  }
}
