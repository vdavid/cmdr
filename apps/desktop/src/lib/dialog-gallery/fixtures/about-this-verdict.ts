/**
 * The `tell-me-about-this` card's AI take, in every state it can show: waiting, streaming, done, stopped, from earlier,
 * each reason it's unavailable, and each way it ends without an answer. Driven by recorded events, ❌ never the E2E
 * fake, so each state holds still in the gallery.
 *
 * Every verdict here is made up, and written the way the prompt asks (two or three plain sentences, no green light).
 */

import type { AboutEvent, Actions, ManagedAiRefusal, VerdictProblem } from '#lib/ipc/bindings.js'
import type { AboutCardData } from '#lib/about-this/about-card.js'
import { card, type Stream } from './about-this-stream'

type Update = AboutEvent['update']

/** A made-up take on the gallery's downloaded disk image, split the way a stream splits it. */
const DMG_VERDICT = [
  'This is the installer disk image for Tessera 2.4.1, ',
  'which macOS recorded as downloaded with Chrome from releases.tessera.example. ',
  'It carries a signature from Tessera Labs AB. ',
  'Removing it doesn’t uninstall Tessera, and Cmdr can’t tell whether the site still offers this version.',
]

const planned: Update = { type: 'verdictPlanned' }
const givenNone: Update = { type: 'verdictGiven', contents: 'none' }
const deltas = (pieces: string[]): Update[] => pieces.map((text): Update => ({ type: 'verdictDelta', text }))
const done: Update = {
  type: 'verdictEnd',
  ending: { kind: 'done' },
  cost: { promptTokens: 1_240, completionTokens: 96, costMicros: 210, priced: true },
}
const failed = (problem: VerdictProblem): Update => ({
  type: 'verdictEnd',
  ending: { kind: 'failed', problem },
  cost: null,
})
const unavailable = (why: Extract<Update, { type: 'verdictUnavailable' }>['why']): Update => ({
  type: 'verdictUnavailable',
  why,
})
const managed = (refusal: ManagedAiRefusal): Update => unavailable({ kind: 'managed', refusal })

/** The streams the verdict states sit on: the downloaded disk image, and the same card while its rows still land. */
export interface VerdictBases {
  name: string
  path: string
  downloaded: Stream
  filling: Stream
  /** A plain document, for the AI-off hint. */
  document: { name: string; path: string; stream: Stream }
}

/**
 * `stream` as it lands with Ask Cmdr on: "Continue in Ask Cmdr" after the facts' own next steps, before removal, as
 * `cmdr_about::actions` orders it. On the disk image the three shown are full, so it heads "More".
 */
function withAskCmdr(stream: Stream): Stream {
  const ask = (actions: Actions): Actions => {
    const all = [...actions.shown, ...actions.more]
    const at = all.findIndex((action) => action.type === 'moveToTrash' || action.type === 'delete')
    all.splice(at === -1 ? all.length : at, 0, { type: 'continueInAskCmdr' })
    return { shown: all.slice(0, actions.shown.length), more: all.slice(actions.shown.length) }
  }
  return {
    ...stream,
    facts: stream.facts.map((landing) => ({
      ...landing,
      derived: { ...landing.derived, actions: ask(landing.derived.actions) },
    })),
  }
}

export function aboutThisVerdictFixtures(bases: VerdictBases): Record<string, AboutCardData> {
  const { name, path, document } = bases
  // A take needs a provider, and these cards show one with Ask Cmdr on too.
  const downloaded = withAskCmdr(bases.downloaded)
  const filling = withAskCmdr(bases.filling)
  const on = (stream: Stream, after: Update[]): AboutCardData => card(name, path, stream, after)
  return {
    'verdict-waiting': on(filling, [planned]),
    'verdict-going-out': on(downloaded, [planned, givenNone]),
    'verdict-streaming': on(downloaded, [planned, givenNone, ...deltas(DMG_VERDICT.slice(0, 2))]),
    'verdict-done': on(downloaded, [planned, givenNone, ...deltas(DMG_VERDICT), done]),
    'verdict-stopped': on(downloaded, [
      planned,
      givenNone,
      ...deltas(DMG_VERDICT.slice(0, 1)),
      { type: 'verdictEnd', ending: { kind: 'stopped' }, cost: null },
    ]),
    'verdict-from-earlier': on(downloaded, [
      planned,
      { type: 'verdictFromEarlier', contents: 'none', text: DMG_VERDICT.join('') },
    ]),
    'verdict-no-answer': on(downloaded, [planned, givenNone, failed('noAnswer')]),
    'verdict-unreachable': on(downloaded, [
      planned,
      givenNone,
      ...deltas(DMG_VERDICT.slice(0, 1)),
      failed('unreachable'),
    ]),
    'verdict-no-consent': on(downloaded, [unavailable({ kind: 'noCloudConsent' })]),
    'verdict-not-configured': on(downloaded, [unavailable({ kind: 'notConfigured' })]),
    'verdict-not-ready': on(downloaded, [unavailable({ kind: 'notReady' })]),
    'verdict-model-off': on(downloaded, [unavailable({ kind: 'modelOff' })]),
    'verdict-managed': on(downloaded, [managed('cloudAiOff')]),
    'verdict-too-many': on(downloaded, [unavailable({ kind: 'tooManyRequests' })]),
    'verdict-local-off': on(downloaded, [unavailable({ kind: 'localOff' })]),
    'verdict-ai-off': card(document.name, document.path, document.stream, [unavailable({ kind: 'aiOff' })]),
  }
}
