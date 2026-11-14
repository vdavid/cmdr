/**
 * The `tell-me-about-this` fixtures for items the card reaches through their volume: a NAS that stopped answering
 * mid-card, a file inside a zip, and a file inside git's history view. Streams, as in `about-this.ts`; every `derived`
 * part is written the way `cmdr-about` shapes it for these facts.
 *
 * Raw data on purpose: dev-only, and every name and path is made up.
 */

import type { Headline } from '#lib/ipc/bindings.js'
import type { AboutCardData } from '#lib/about-this/about-card.js'
import { card, day, derived, identity, kindByName, plan, routine, type Stream } from './about-this-stream'

// ── A NAS that stopped answering while the card gathered ───────────────────────

const SILENT = 'Tax return 2025.pdf'
const silentHeadline: Headline = {
  what: { type: 'file', kind: 'pdf', extension: 'pdf' },
  size: null,
  lead: { template: 'plain' },
}
const silentDerived = derived(silentHeadline, [{ type: 'cantTell' }])

const stoppedAnswering: Stream = {
  plan: plan(
    'file',
    [
      { section: 'origin', kinds: ['download'] },
      { section: 'nature', kinds: ['identity', 'realType'] },
    ],
    { volume: 'Naspolya', limit: 'someFacts' },
  ),
  first: silentDerived,
  stopped: true,
  facts: [
    // The name's claim asks the share nothing, so it stands while the share is silent.
    { fact: kindByName('pdf', 'pdf', 'volumeCantTell'), derived: silentDerived },
    { fact: { kind: 'identity', fact: routine('volumeStoppedAnswering') }, derived: silentDerived },
    { fact: { kind: 'download', fact: routine('volumeStoppedAnswering') }, derived: silentDerived },
  ],
  settled: true,
}

// ── A photo inside a zip: the archive's name and its record ────────────────────

const ZIP = 'Holiday photos.zip'
const ENTRY = 'IMG_0412.jpg'
const entryHeadline: Headline = {
  what: { type: 'archiveEntry' },
  size: { bytes: 2_412_000, atLeast: false },
  lead: { template: 'insideArchive', archive: ZIP },
}

const archiveEntry: Stream = {
  plan: plan('archiveEntry', [
    { section: 'origin', kinds: ['download'] },
    { section: 'nature', kinds: ['identity', 'realType'] },
  ]),
  first: derived({ ...entryHeadline, size: null, lead: { template: 'plain' } }, [{ type: 'cantTell' }]),
  facts: [
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
                  site: { type: 'domain', host: 'photos.example' },
                  url: 'https://photos.example/share/holiday.zip',
                },
              ],
            },
            downloadedAt: day('2026-08-20'),
            downloadedBy: 'Safari',
            archive: ZIP,
          },
        },
      },
      derived: derived({ ...entryHeadline, size: null }, [{ type: 'cantTell' }]),
    },
    { fact: identity(ENTRY, 2_412_000, '2026-08-14'), derived: derived(entryHeadline, [{ type: 'cantTell' }]) },
    { fact: kindByName('image', 'jpg', 'volumeCantTell'), derived: derived(entryHeadline, [{ type: 'cantTell' }]) },
  ],
  settled: true,
}

// ── A file as a commit has it, in git's history view ───────────────────────────

const REPO = '/Users/test_user_1/projects/tessera'
const COMMIT = '1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d'
const snapshotHeadline: Headline = {
  what: { type: 'file', kind: 'text', extension: 'md' },
  size: { bytes: 4_210, atLeast: false },
  lead: { template: 'fromGitHistory', kind: 'commit', name: COMMIT },
}
const inHistory = derived(snapshotHeadline, [{ type: 'inGitHistory' }])

const gitSnapshot: Stream = {
  plan: plan('file', [
    { section: 'origin', kinds: ['gitSnapshot'] },
    { section: 'nature', kinds: ['identity', 'realType'] },
  ]),
  first: derived({ ...snapshotHeadline, size: null, lead: { template: 'plain' } }, [{ type: 'inGitHistory' }]),
  facts: [
    {
      fact: identity('README.md', 4_210, '2026-10-30'),
      derived: derived({ ...snapshotHeadline, lead: { template: 'plain' } }, [{ type: 'inGitHistory' }]),
    },
    {
      fact: {
        kind: 'gitSnapshot',
        fact: {
          state: 'known',
          value: { repoRoot: REPO, kind: 'commit', name: COMMIT, lastChanged: day('2026-10-30') },
        },
      },
      derived: inHistory,
    },
    { fact: kindByName('text', 'md', 'volumeCantTell'), derived: inHistory },
  ],
  settled: true,
}

// ── A zip on a share: Cmdr reads no bytes there, so its kind is its name's claim ─

const SHARED_ZIP = 'Scans 2025.zip'
const sharedZipHeadline: Headline = {
  what: { type: 'file', kind: 'archive', extension: 'zip' },
  size: { bytes: 48_120_000, atLeast: false },
  lead: { template: 'plain' },
}
const sharedZipDerived = derived(sharedZipHeadline, [{ type: 'cantTell' }])

const shareFile: Stream = {
  plan: plan('file', [{ section: 'nature', kinds: ['identity', 'realType'] }], {
    volume: 'Naspolya',
    limit: 'someFacts',
  }),
  first: derived({ ...sharedZipHeadline, size: null }, [{ type: 'cantTell' }]),
  facts: [
    { fact: kindByName('archive', 'zip', 'volumeCantTell'), derived: sharedZipDerived },
    { fact: identity(SHARED_ZIP, 48_120_000, '2026-02-11'), derived: sharedZipDerived },
  ],
  settled: true,
}

/** Keyed by the `tell-me-about-this` entry's state ids in `gallery-registry.ts`. */
export const aboutThisDegradedFixtures: Record<string, AboutCardData> = {
  'stopped-answering': card(SILENT, `/Volumes/naspi/papers/${SILENT}`, stoppedAnswering),
  'archive-entry': card(ENTRY, `/Users/test_user_1/Downloads/${ZIP}/${ENTRY}`, archiveEntry),
  'git-snapshot': card('README.md', `${REPO}/.git/commits/${COMMIT}/README.md`, gitSnapshot),
  'share-file': card(SHARED_ZIP, `/Volumes/naspi/papers/${SHARED_ZIP}`, shareFile),
}
