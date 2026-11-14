/**
 * The card's AI take ("the verdict"), as its events left it, and the one reducer that folds them in.
 *
 * Pure: no runes, no IPC. `applyAboutEvent` hands every verdict event here, so the live card and the gallery's
 * fixtures build the same thing.
 *
 * ❗ The verdict is untrusted text: a hostile file name can steer it. It's kept here as a plain string and rendered as
 * a text node, and ❌ nothing else on the card reads it: the rows, the removal line, the actions, and focus are the
 * same with and without it.
 */

import type {
  AboutEvent,
  ModelTrust,
  VerdictContents,
  VerdictCost,
  VerdictEnding,
  VerdictProblem,
  VerdictUnavailable,
} from '#lib/ipc/bindings.js'

/** The verdict the card got: none at all, a typed reason it can't have one, or one being written. */
export type AboutVerdict =
  | { status: 'unavailable'; why: VerdictUnavailable }
  | {
      status: 'planned'
      /** How far the model writing it can be trusted: the block warns below `trusted`. Rust decides it. */
      trust: ModelTrust
      /** What of the item's contents went out with the facts, from the moment the call went out. */
      given: VerdictContents | null
      /** The text so far. Plain text, ❌ never Markdown or markup. */
      text: string
      /** How it ended, once it did. */
      ending: VerdictEnding | null
      /** What it cost, when the provider reported it. */
      cost: VerdictCost | null
      /** It's a finished take from earlier in this session: no call was made. */
      fromEarlier: boolean
    }

type AboutUpdate = AboutEvent['update']
export type VerdictUpdate = Extract<
  AboutUpdate,
  {
    type:
      | 'verdictPlanned'
      | 'verdictUnavailable'
      | 'verdictGiven'
      | 'verdictDelta'
      | 'verdictEnd'
      | 'verdictFromEarlier'
  }
>

const VERDICT_UPDATES: ReadonlySet<AboutUpdate['type']> = new Set<VerdictUpdate['type']>([
  'verdictPlanned',
  'verdictUnavailable',
  'verdictGiven',
  'verdictDelta',
  'verdictEnd',
  'verdictFromEarlier',
])

export function isVerdictUpdate(update: AboutUpdate): update is VerdictUpdate {
  return VERDICT_UPDATES.has(update.type)
}

/**
 * Folds one verdict event in. Returns the same value when the event changes nothing.
 *
 * Drops, on purpose: a second `verdictPlanned` or `verdictUnavailable` (the answer comes once, right after the plan),
 * anything for a verdict that isn't planned, a second `verdictGiven`, and anything after its end.
 */
export function applyVerdictUpdate(verdict: AboutVerdict | null, update: VerdictUpdate): AboutVerdict | null {
  if (update.type === 'verdictPlanned') {
    if (verdict !== null) return verdict
    return {
      status: 'planned',
      trust: update.trust,
      given: null,
      text: '',
      ending: null,
      cost: null,
      fromEarlier: false,
    }
  }
  if (update.type === 'verdictUnavailable') {
    return verdict === null ? { status: 'unavailable', why: update.why } : verdict
  }
  if (verdict?.status !== 'planned' || verdict.ending !== null) return verdict
  switch (update.type) {
    case 'verdictGiven':
      return verdict.given === null ? { ...verdict, given: update.contents } : verdict
    case 'verdictDelta':
      return update.text === '' ? verdict : { ...verdict, text: verdict.text + update.text }
    case 'verdictEnd':
      return { ...verdict, ending: update.ending, cost: update.cost }
    case 'verdictFromEarlier':
      if (verdict.text !== '') return verdict
      return { ...verdict, given: update.contents, text: update.text, ending: { kind: 'done' }, fromEarlier: true }
  }
}

/** What the block shows, from a planned verdict. */
export type VerdictPhase =
  /** Planned, nothing written yet: the facts may still be landing, or the call is on its way. */
  | 'waiting'
  /** Text is arriving. */
  | 'streaming'
  | 'done'
  /** Stopped before its end: the text so far stands. */
  | 'stopped'
  /** A finished take from earlier in this session. */
  | 'fromEarlier'
  /** The provider couldn't answer (or wrote nothing): `problem` says which. */
  | 'failed'

export function verdictPhase(verdict: Extract<AboutVerdict, { status: 'planned' }>): VerdictPhase {
  const { ending } = verdict
  if (ending === null) return verdict.text === '' ? 'waiting' : 'streaming'
  if (verdict.fromEarlier) return 'fromEarlier'
  return ending.kind
}

/** Why a failed verdict failed, or `null` when it didn't. */
export function verdictProblem(verdict: Extract<AboutVerdict, { status: 'planned' }>): VerdictProblem | null {
  return verdict.ending?.kind === 'failed' ? verdict.ending.problem : null
}

/**
 * Where the card puts its AI take, which also picks its layout:
 *
 * - `column`: the take's own column beside the facts. A planned take, and the reasons that are AI being on but not
 *   answering this time (the local server not running, the organization's policy, too many in a minute), which say so
 *   in the take's place.
 * - `hint`: one column of facts and a dismissable line under the footer saying AI adds a deeper read, with the way to
 *   the setting. The reasons a setting fixes: AI off, cloud AI not allowed yet, no key.
 * - `none`: one column and nothing about AI. The person turned the take off themselves.
 */
export type VerdictPlace = 'column' | 'hint' | 'none'

/** The reasons a setting fixes, which the card says in its hint line. */
export type HintWhy = Extract<VerdictUnavailable, { kind: 'aiOff' | 'noCloudConsent' | 'notConfigured' }>

export function verdictPlace(verdict: AboutVerdict | null): VerdictPlace {
  if (verdict === null) return 'none'
  if (verdict.status === 'planned') return 'column'
  return placeOfUnavailable(verdict.why)
}

function placeOfUnavailable(why: VerdictUnavailable): VerdictPlace {
  switch (why.kind) {
    case 'aiOff':
    case 'noCloudConsent':
    case 'notConfigured':
      return 'hint'
    case 'turnedOff':
      return 'none'
    case 'notReady':
    case 'managed':
    case 'tooManyRequests':
      return 'column'
  }
}

/** The hint line's reason, when the card shows one. */
export function hintWhy(verdict: AboutVerdict | null): HintWhy | null {
  if (verdict?.status !== 'unavailable') return null
  const { why } = verdict
  return why.kind === 'aiOff' || why.kind === 'noCloudConsent' || why.kind === 'notConfigured' ? why : null
}
