/**
 * Fixtures for the `tell-me-about-this` card (`#lib/about-this/AboutThisDialog.svelte`).
 *
 * Each state is a STREAM of the backend's events folded through the card's own reducer (`applyAboutEvent`), so a
 * preview is exactly what that stream renders live. The `derived` parts (headline, removal) are written the way
 * `cmdr-about` derives them for these facts; nothing here derives them.
 *
 * Raw data on purpose: this module is dev-only. Every host, name, and path is made up (❌ no real download tokens).
 */

import type {
  AboutEvent,
  AboutFact,
  AboutPlan,
  Derived,
  Fact,
  Headline,
  PlannedSection,
  RemovalConsequence,
  UnknownWhy,
} from '#lib/ipc/bindings.js'
import { applyAboutEvent, emptyCard, type AboutCardData } from '#lib/about-this/about-card.js'

const REQUEST = 'gallery'

/** Seconds since the epoch for a calendar day, at noon UTC. */
function day(iso: string): number {
  return Math.floor(Date.parse(`${iso}T12:00:00Z`) / 1000)
}

function routine(why: UnknownWhy): Fact<never> {
  return { state: 'unknown', value: { why, prominence: 'routine' } }
}
function notable(why: UnknownWhy): Fact<never> {
  return { state: 'unknown', value: { why, prominence: 'notable' } }
}

function derived(headline: Headline, shown: RemovalConsequence[], more: RemovalConsequence[] = []): Derived {
  return { headline, removal: { shown, more } }
}

interface Stream {
  plan: AboutPlan
  /** The headline and removal as they stand with every row pending. */
  first: Derived
  /** Each landing, with what the headline and removal say after it. */
  facts: { fact: AboutFact; derived: Derived }[]
  settled: boolean
}

function card(name: string, path: string, stream: Stream): AboutCardData {
  const events: AboutEvent[] = [
    { requestId: REQUEST, update: { type: 'planned', plan: stream.plan, derived: stream.first } },
    ...stream.facts.map((landing): AboutEvent => ({
      requestId: REQUEST,
      update: { type: 'fact', fact: landing.fact, derived: landing.derived },
    })),
  ]
  if (stream.settled) events.push({ requestId: REQUEST, update: { type: 'factsSettled' } })
  return events.reduce(applyAboutEvent, emptyCard(REQUEST, { path, volumeId: 'root', name }))
}

function plan(
  target: AboutPlan['target'],
  sections: PlannedSection[],
  volumeNote: AboutPlan['volumeNote'] = null,
): AboutPlan {
  return { target, sections: [...sections, { section: 'removal', kinds: [] }], volumeNote }
}

function identity(
  name: string,
  size: number | null,
  modified: string,
  extra: { hardLinks?: number; symlinkTarget?: string } = {},
): AboutFact {
  return {
    kind: 'identity',
    fact: {
      state: 'known',
      value: {
        name,
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

// ── A downloaded disk image: every origin fact known ───────────────────────────

const DMG = 'Tessera-2.4.1-arm64.dmg'
const dmgHeadline: Headline = {
  what: { type: 'file', kind: 'diskImage', extension: 'dmg' },
  size: { bytes: 412_318_720, atLeast: false },
  lead: { template: 'downloadedFrom', host: 'releases.tessera.example', by: 'Chrome', at: day('2026-03-02') },
}
const dmgRemoval = derived(dmgHeadline, [
  { type: 'userDocument' },
  { type: 'recordedDownload', host: 'releases.tessera.example' },
])

const downloaded: Stream = {
  plan: plan('file', [
    { section: 'origin', kinds: ['download', 'cmdrOperation'] },
    { section: 'nature', kinds: ['identity', 'realType', 'signature'] },
    { section: 'use', kinds: ['lastOpened', 'defaultApp'] },
  ]),
  first: derived({ ...dmgHeadline, size: null, lead: { template: 'plain' } }, [{ type: 'cantTell' }]),
  facts: [
    { fact: identity(DMG, 412_318_720, '2026-03-02'), derived: dmgRemoval },
    {
      fact: {
        kind: 'download',
        fact: {
          state: 'known',
          value: {
            sources: {
              state: 'known',
              value: [
                {
                  type: 'web',
                  site: { type: 'domain', host: 'releases.tessera.example' },
                  url: 'https://releases.tessera.example/download/2.4.1/Tessera-2.4.1-arm64.dmg',
                },
                {
                  type: 'web',
                  site: { type: 'domain', host: 'tessera.example' },
                  url: 'https://tessera.example/download',
                },
              ],
            },
            downloadedAt: day('2026-03-02'),
            downloadedBy: 'Chrome',
          },
        },
      },
      derived: dmgRemoval,
    },
    { fact: { kind: 'cmdrOperation', fact: routine('noRecord') }, derived: dmgRemoval },
    {
      fact: {
        kind: 'realType',
        fact: {
          state: 'known',
          value: {
            sniffed: { kind: 'diskImage', format: 'dmg' },
            claimed: { extension: 'dmg', kind: 'diskImage' },
            agreement: 'matches',
          },
        },
      },
      derived: dmgRemoval,
    },
    {
      fact: {
        kind: 'signature',
        fact: {
          state: 'known',
          value: {
            signer: { type: 'developerId', team: 'T3SS3RA001', org: 'Tessera Labs AB' },
            ticket: 'notRead',
            checked: { type: 'notChecked', why: 'notSupported' },
          },
        },
      },
      derived: dmgRemoval,
    },
    { fact: { kind: 'lastOpened', fact: routine('noRecord') }, derived: dmgRemoval },
    {
      fact: {
        kind: 'defaultApp',
        fact: { state: 'known', value: { name: 'DiskImageMounter', id: 'com.apple.DiskImageMounter' } },
      },
      derived: dmgRemoval,
    },
  ],
  settled: true,
}

// ── A plain document: identity and use, origin collapsed ───────────────────────

const DOC = 'Budget 2026.numbers'
const docHeadline: Headline = {
  what: { type: 'file', kind: 'document', extension: 'numbers' },
  size: { bytes: 2_183_412, atLeast: false },
  lead: { template: 'lastOpened', at: day('2026-10-02') },
}
const docRemoval = derived(docHeadline, [{ type: 'userDocument' }])

const plainDocument: Stream = {
  plan: plan('file', [
    { section: 'origin', kinds: ['download', 'cmdrOperation'] },
    { section: 'nature', kinds: ['identity', 'realType'] },
    { section: 'use', kinds: ['lastOpened', 'defaultApp'] },
  ]),
  first: derived({ ...docHeadline, size: null, lead: { template: 'plain' } }, [{ type: 'cantTell' }]),
  facts: [
    { fact: identity(DOC, 2_183_412, '2026-09-28'), derived: docRemoval },
    { fact: { kind: 'download', fact: routine('noRecord') }, derived: docRemoval },
    { fact: { kind: 'cmdrOperation', fact: routine('noRecord') }, derived: docRemoval },
    {
      fact: {
        kind: 'realType',
        fact: {
          state: 'known',
          value: {
            sniffed: { kind: 'document', format: 'zip' },
            claimed: { extension: 'numbers', kind: 'document' },
            agreement: 'matches',
          },
        },
      },
      derived: docRemoval,
    },
    {
      fact: { kind: 'lastOpened', fact: { state: 'known', value: { lastOpened: day('2026-10-02'), useCount: 14 } } },
      derived: docRemoval,
    },
    {
      fact: { kind: 'defaultApp', fact: { state: 'known', value: { name: 'Numbers', id: 'com.apple.Numbers' } } },
      derived: docRemoval,
    },
  ],
  settled: true,
}

// ── Still filling: the same document a moment after it opened ──────────────────

const stillFilling: Stream = { ...plainDocument, facts: plainDocument.facts.slice(0, 2), settled: false }

// ── A folder in a repo: size, importance, git ──────────────────────────────────

const FOLDER = 'website'
const folderHeadline: Headline = {
  what: { type: 'folder' },
  size: { bytes: 48_211_904, atLeast: true },
  lead: { template: 'plain' },
}
const folderRemoval = derived(folderHeadline, [{ type: 'uncommittedWork' }, { type: 'userDocument' }])

const folder: Stream = {
  plan: plan('folder', [
    { section: 'nature', kinds: ['identity', 'recursiveSize'] },
    { section: 'origin', kinds: ['cmdrOperation'] },
    { section: 'use', kinds: ['importance'] },
    { section: 'context', kinds: ['git'] },
  ]),
  first: derived({ ...folderHeadline, size: null }, [{ type: 'cantTell' }]),
  facts: [
    { fact: identity(FOLDER, null, '2026-11-10'), derived: folderRemoval },
    {
      fact: {
        kind: 'recursiveSize',
        fact: {
          state: 'known',
          value: {
            logical: 48_211_904,
            physical: 52_428_800,
            files: 1_284,
            folders: 211,
            coverage: 'lowerBound',
            updating: false,
            hasSymlinks: true,
          },
        },
      },
      derived: folderRemoval,
    },
    { fact: { kind: 'cmdrOperation', fact: routine('noRecord') }, derived: folderRemoval },
    {
      fact: {
        kind: 'importance',
        fact: {
          state: 'known',
          value: {
            band: 'high',
            stale: false,
            reasons: ['projectMarker', 'mtimeRecency', 'visitActivity'],
            visits: 37,
          },
        },
      },
      derived: folderRemoval,
    },
    {
      fact: {
        kind: 'git',
        fact: {
          state: 'known',
          value: {
            repoRoot: '/Users/test_user_1/projects/website',
            branch: 'main',
            status: { state: 'known', value: { type: 'tracked', state: 'modified' } },
          },
        },
      },
      derived: folderRemoval,
    },
  ],
  settled: true,
}

// ── An app: bundle, signature with a stapled ticket ────────────────────────────

const APP = 'Tessera.app'
const appHeadline: Headline = {
  what: { type: 'app', version: '2.4.1' },
  size: null,
  lead: { template: 'lastOpened', at: day('2026-11-12') },
}
const appRemoval = derived(appHeadline, [{ type: 'removesApp', dataLeft: routine('volumeCantTell') }])

const app: Stream = {
  plan: plan('appBundle', [
    { section: 'nature', kinds: ['identity', 'bundle', 'signature'] },
    { section: 'origin', kinds: ['download', 'cmdrOperation'] },
    { section: 'use', kinds: ['lastOpened'] },
  ]),
  first: derived({ ...appHeadline, lead: { template: 'plain' } }, [
    { type: 'removesApp', dataLeft: { state: 'pending' } },
  ]),
  facts: [
    { fact: identity(APP, null, '2026-03-02'), derived: appRemoval },
    {
      fact: {
        kind: 'bundle',
        fact: { state: 'known', value: { bundleId: 'example.tessera.mac', displayName: 'Tessera', version: '2.4.1' } },
      },
      derived: appRemoval,
    },
    {
      fact: {
        kind: 'signature',
        fact: {
          state: 'known',
          value: {
            signer: { type: 'developerId', team: 'T3SS3RA001', org: 'Tessera Labs AB' },
            ticket: 'present',
            checked: { type: 'executableMatches' },
          },
        },
      },
      derived: appRemoval,
    },
    {
      fact: {
        kind: 'download',
        fact: {
          state: 'known',
          value: { sources: routine('noRecord'), downloadedAt: day('2026-03-02'), downloadedBy: 'Chrome' },
        },
      },
      derived: appRemoval,
    },
    { fact: { kind: 'cmdrOperation', fact: routine('noRecord') }, derived: appRemoval },
    {
      fact: { kind: 'lastOpened', fact: { state: 'known', value: { lastOpened: day('2026-11-12'), useCount: null } } },
      derived: appRemoval,
    },
  ],
  settled: true,
}

// ── A program named like a photo ───────────────────────────────────────────────

const LIAR = 'holiday-photo.jpg'
const liarHeadline: Headline = {
  what: { type: 'file', kind: 'machO', extension: 'jpg' },
  size: { bytes: 88_064, atLeast: false },
  lead: { template: 'receivedByAirDrop', at: day('2026-11-01') },
}
const liarRemoval = derived(liarHeadline, [{ type: 'userDocument' }])

const mismatch: Stream = {
  plan: plan('file', [
    { section: 'origin', kinds: ['download', 'cmdrOperation'] },
    { section: 'nature', kinds: ['identity', 'realType', 'signature'] },
    { section: 'use', kinds: ['lastOpened', 'defaultApp'] },
  ]),
  first: derived({ ...liarHeadline, lead: { template: 'plain' } }, [{ type: 'cantTell' }]),
  facts: [
    { fact: identity(LIAR, 88_064, '2026-11-01'), derived: liarRemoval },
    {
      fact: {
        kind: 'download',
        fact: {
          state: 'known',
          value: {
            sources: { state: 'known', value: [{ type: 'airDrop', sender: 'Test Phone' }] },
            downloadedAt: day('2026-11-01'),
            downloadedBy: 'sharingd',
          },
        },
      },
      derived: liarRemoval,
    },
    {
      fact: {
        kind: 'cmdrOperation',
        fact: {
          state: 'known',
          value: {
            kind: 'copy',
            at: day('2026-11-02'),
            initiator: 'you',
            sourcePath: '/Volumes/Testnas/inbox/holiday-photo.jpg',
            sourceVolume: 'Testnas',
          },
        },
      },
      derived: liarRemoval,
    },
    {
      fact: {
        kind: 'realType',
        fact: {
          state: 'known',
          value: {
            sniffed: { kind: 'machO', format: 'mach' },
            claimed: { extension: 'jpg', kind: 'image' },
            agreement: 'mismatch',
          },
        },
      },
      derived: liarRemoval,
    },
    {
      fact: {
        kind: 'signature',
        fact: {
          state: 'known',
          value: {
            signer: { type: 'unsigned' },
            ticket: 'absent',
            checked: { type: 'notChecked', why: 'nothingToCheck' },
          },
        },
      },
      derived: liarRemoval,
    },
    { fact: { kind: 'lastOpened', fact: routine('noRecord') }, derived: liarRemoval },
    {
      fact: { kind: 'defaultApp', fact: { state: 'known', value: { name: 'Preview', id: 'com.apple.Preview' } } },
      derived: liarRemoval,
    },
  ],
  settled: true,
}

// ── Every Notable reason, over two cards ───────────────────────────────────────

const libraryHeadline: Headline = { what: { type: 'folder' }, size: null, lead: { template: 'plain' } }
const libraryRemoval = derived(libraryHeadline, [{ type: 'cantTell' }])

const notableA: Stream = {
  plan: plan('folder', [
    { section: 'nature', kinds: ['identity', 'recursiveSize'] },
    { section: 'origin', kinds: ['download', 'cmdrOperation'] },
    { section: 'use', kinds: ['lastOpened', 'importance'] },
    { section: 'context', kinds: ['owner', 'git'] },
  ]),
  first: derived(libraryHeadline, [{ type: 'cantTell' }]),
  facts: [
    { fact: identity('Google', null, '2026-08-14'), derived: libraryRemoval },
    { fact: { kind: 'recursiveSize', fact: notable('notIndexed') }, derived: libraryRemoval },
    { fact: { kind: 'download', fact: notable('timedOut') }, derived: libraryRemoval },
    { fact: { kind: 'cmdrOperation', fact: notable('nodeChangedSince') }, derived: libraryRemoval },
    { fact: { kind: 'lastOpened', fact: notable('permissionDenied') }, derived: libraryRemoval },
    { fact: { kind: 'importance', fact: notable('unparseable') }, derived: libraryRemoval },
    { fact: { kind: 'owner', fact: notable('ambiguous') }, derived: libraryRemoval },
    {
      fact: {
        kind: 'git',
        fact: {
          state: 'known',
          value: { repoRoot: '/Users/test_user_1/Library', branch: null, status: notable('timedOut') },
        },
      },
      derived: libraryRemoval,
    },
  ],
  settled: true,
}

const notableB: Stream = {
  plan: plan('folder', [
    { section: 'nature', kinds: ['identity', 'recursiveSize'] },
    { section: 'origin', kinds: ['download'] },
    { section: 'use', kinds: ['importance'] },
    { section: 'context', kinds: ['owner'] },
  ]),
  first: derived(libraryHeadline, [{ type: 'cantTell' }]),
  facts: [
    { fact: identity('Tessera Helper', null, '2026-05-30'), derived: libraryRemoval },
    { fact: { kind: 'recursiveSize', fact: notable('unreachable') }, derived: libraryRemoval },
    { fact: { kind: 'download', fact: notable('protected') }, derived: libraryRemoval },
    { fact: { kind: 'importance', fact: notable('refusedInBackground') }, derived: libraryRemoval },
    { fact: { kind: 'owner', fact: notable('libraryNameMatchesNoApp') }, derived: libraryRemoval },
  ],
  settled: true,
}

// ── On a NAS: the volume note, then identity only ──────────────────────────────

const REMOTE = 'IMG_2041.HEIC'
const remoteHeadline: Headline = {
  what: { type: 'file', kind: 'image', extension: 'heic' },
  size: { bytes: 3_604_480, atLeast: false },
  lead: { template: 'plain' },
}

const remote: Stream = {
  plan: plan('file', [{ section: 'nature', kinds: ['identity'] }], { volume: 'Testnas', limit: 'identityOnly' }),
  first: derived({ ...remoteHeadline, size: null }, [{ type: 'cantTell' }]),
  facts: [
    { fact: identity(REMOTE, 3_604_480, '2026-07-14'), derived: derived(remoteHeadline, [{ type: 'cantTell' }]) },
  ],
  settled: true,
}

// ── A hard-linked file with more consequences than fit ─────────────────────────

const LINKED = 'notes.md'
const linkedHeadline: Headline = {
  what: { type: 'file', kind: 'text', extension: 'md' },
  size: { bytes: 18_220, atLeast: false },
  lead: { template: 'plain' },
}
const linkedRemoval = derived(
  linkedHeadline,
  [{ type: 'untrackedNotInGit' }, { type: 'userDocument' }],
  [{ type: 'otherNamesRemain', others: 1 }],
)

const hardLinked: Stream = {
  plan: plan('file', [
    { section: 'nature', kinds: ['identity', 'realType'] },
    { section: 'context', kinds: ['git'] },
  ]),
  first: derived({ ...linkedHeadline, size: null }, [{ type: 'cantTell' }]),
  facts: [
    { fact: identity(LINKED, 18_220, '2026-11-11', { hardLinks: 2 }), derived: linkedRemoval },
    {
      fact: {
        kind: 'realType',
        fact: {
          state: 'known',
          value: {
            sniffed: { kind: 'unrecognized', format: null },
            claimed: { extension: 'md', kind: 'text' },
            agreement: 'cantTell',
          },
        },
      },
      derived: linkedRemoval,
    },
    {
      fact: {
        kind: 'git',
        fact: {
          state: 'known',
          value: {
            repoRoot: '/Users/test_user_1/notes',
            branch: 'main',
            status: { state: 'known', value: { type: 'untracked' } },
          },
        },
      },
      derived: linkedRemoval,
    },
  ],
  settled: true,
}

/** Keyed by the `tell-me-about-this` entry's state ids in `gallery-registry.ts`. */
export const aboutThisFixtures: Record<string, AboutCardData | undefined> = {
  downloaded: card(DMG, `/Users/test_user_1/Downloads/${DMG}`, downloaded),
  'plain-document': card(DOC, `/Users/test_user_1/Documents/${DOC}`, plainDocument),
  'still-filling': card(DOC, `/Users/test_user_1/Documents/${DOC}`, stillFilling),
  folder: card(FOLDER, '/Users/test_user_1/projects/website', folder),
  app: card(APP, `/Applications/${APP}`, app),
  mismatch: card(LIAR, `/Users/test_user_1/Desktop/${LIAR}`, mismatch),
  'notable-a': card('Google', '/Users/test_user_1/Library/Application Support/Google', notableA),
  'notable-b': card('Tessera Helper', '/Users/test_user_1/Library/Application Support/Tessera Helper', notableB),
  remote: card(REMOTE, '/Volumes/Testnas/photos/IMG_2041.HEIC', remote),
  'more-consequences': card(LINKED, '/Users/test_user_1/notes/notes.md', hardLinked),
  unavailable: {
    ...emptyCard(REQUEST, { path: '/Volumes/Gone/old.txt', volumeId: 'gone', name: 'old.txt' }),
    unavailable: 'notFound',
    settled: true,
  },
}
