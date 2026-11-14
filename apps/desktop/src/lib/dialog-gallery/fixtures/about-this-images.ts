/**
 * The `tell-me-about-this` fixtures for a picture's "What it shows" row: what the image index recognized (tags and
 * text), a picture it hasn't reached, image indexing turned off, and a picture it found nothing in. Streams, as in
 * `about-this.ts`; every `derived` part is written the way `cmdr-about` shapes it for these facts.
 *
 * Raw data on purpose: dev-only, and every name, path, and line of text is made up.
 */

import type { AboutFact, Fact, Headline, ImageContents } from '#lib/ipc/bindings.js'
import type { AboutCardData } from '#lib/about-this/about-card.js'
import { card, derived, identity, plan, routine, type Stream } from './about-this-stream'

const PHOTO = 'IMG_4471.jpg'
const PATH = `/Users/test_user_1/Pictures/${PHOTO}`

const headline: Headline = {
  what: { type: 'file', kind: 'image', extension: 'jpg' },
  size: { bytes: 2_918_400, atLeast: false },
  lead: { template: 'plain' },
}
const settledDerived = derived(headline, [{ type: 'userDocument' }], [], {
  shown: [{ type: 'openWithDefaultApp', app: 'Preview' }, { type: 'showInFileManager' }, { type: 'moveToTrash' }],
  more: [],
})

/** The picture's card with its "What it shows" row answering `shows`. */
function picture(shows: Fact<ImageContents>): Stream {
  const realType: AboutFact = {
    kind: 'realType',
    fact: {
      state: 'known',
      value: {
        sniffed: { kind: 'image', format: 'jpg' },
        claimed: { extension: 'jpg', kind: 'image' },
        agreement: 'matches',
      },
    },
  }
  return {
    plan: plan('file', [
      { section: 'origin', kinds: ['download', 'cmdrOperation'] },
      { section: 'nature', kinds: ['identity', 'realType', 'imageContents'] },
      { section: 'use', kinds: ['defaultApp'] },
    ]),
    first: derived({ ...headline, size: null }, [{ type: 'cantTell' }]),
    facts: [
      { fact: identity(PHOTO, 2_918_400, '2026-08-17'), derived: settledDerived },
      { fact: { kind: 'download', fact: routine('noRecord') }, derived: settledDerived },
      { fact: { kind: 'cmdrOperation', fact: routine('noRecord') }, derived: settledDerived },
      { fact: realType, derived: settledDerived },
      { fact: { kind: 'imageContents', fact: shows }, derived: settledDerived },
      {
        fact: { kind: 'defaultApp', fact: { state: 'known', value: { name: 'Preview', id: 'com.apple.Preview' } } },
        derived: settledDerived,
      },
    ],
    settled: true,
  }
}

const shows: Fact<ImageContents> = {
  state: 'known',
  value: {
    tags: ['sign', 'text', 'outdoor', 'storefront'],
    textLines: ['Corner Bakery', 'Open every day 7–19', 'Fresh bread from 7'],
    moreText: true,
  },
}

const nothingFound: Fact<ImageContents> = {
  state: 'known',
  value: { tags: [], textLines: [], moreText: false },
}

/** Keyed by the `tell-me-about-this` entry's state ids in `gallery-registry.ts`. */
export const aboutThisImageFixtures: Record<string, AboutCardData> = {
  'picture-shows': card(PHOTO, PATH, picture(shows)),
  'picture-not-indexed': card(PHOTO, PATH, picture(routine('notIndexed'))),
  'picture-indexing-off': card(PHOTO, PATH, picture(routine('indexingOff'))),
  'picture-nothing-found': card(PHOTO, PATH, picture(nothingFound)),
}
