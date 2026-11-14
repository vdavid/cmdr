/**
 * The `tell-me-about-this` fixtures for folders and apps: What's inside (a big folder in a repo, `node_modules`, an
 * empty folder, one the index hasn't listed) and an app with its data in the Library. Streams, as in `about-this.ts`;
 * the digest, the kind mix, and every `derived` part are written the way `cmdr-about` shapes them for these facts.
 *
 * Raw data on purpose: dev-only, and every name and path is made up.
 */

import type {
  AboutFact,
  AppData,
  FolderDigest,
  Headline,
  KindCount,
  RecursiveSize,
  SizeCoverage,
  SizedChild,
} from '#lib/ipc/bindings.js'
import type { AboutCardData } from '#lib/about-this/about-card.js'
import { joinPath } from '#lib/path/grammar.js'
import { card, day, derived, identity, plan, routine, type Stream } from './about-this-stream'

const HOME = '/Users/test_user_1'

function total(logical: number, coverage: SizeCoverage = 'exact', files = 1, folders = 0): RecursiveSize {
  return {
    logical,
    physical: Math.ceil(logical / 4096) * 4096,
    files,
    folders,
    coverage,
    updating: false,
    hasSymlinks: false,
  }
}

function child(
  folder: string,
  name: string,
  bytes: number | null,
  isFolder: boolean,
  coverage?: SizeCoverage,
): SizedChild {
  return {
    name,
    path: joinPath(folder, name),
    isFolder,
    size:
      bytes === null
        ? routine('notIndexed')
        : { state: 'known', value: total(bytes, coverage, isFolder ? 120 : 1, isFolder ? 8 : 0) },
  }
}

function sizeFact(size: RecursiveSize): AboutFact {
  return { kind: 'recursiveSize', fact: { state: 'known', value: size } }
}

/** A digest of a folder the index listed: its children, written flat for reading. */
function digestFact(listed: {
  directChildren: number
  biggestChildren: SizedChild[]
  kindMix: KindCount[]
  largestFiles: FolderDigest['largestFiles']
  unchangedForAYear: FolderDigest['unchangedForAYear']
  class: FolderDigest['class']
}): AboutFact {
  const digest: FolderDigest = {
    class: listed.class,
    children: {
      state: 'known',
      value: { count: listed.directChildren, biggest: listed.biggestChildren, kindMix: listed.kindMix },
    },
    largestFiles: listed.largestFiles,
    unchangedForAYear: listed.unchangedForAYear,
  }
  return { kind: 'folderDigest', fact: { state: 'known', value: digest } }
}

const noRecords: AboutFact[] = [
  { kind: 'download', fact: routine('noRecord') },
  { kind: 'cmdrOperation', fact: routine('noRecord') },
]

/** A folder's sections in the order `cmdr-about::card::plan` gives a folder: What's inside first. */
function folderPlan(context: AboutFact['kind'][] = []) {
  return plan('folder', [
    { section: 'contents', kinds: ['folderDigest'] },
    { section: 'nature', kinds: ['identity', 'recursiveSize'] },
    ...(context.length > 0 ? [{ section: 'context' as const, kinds: context }] : []),
    { section: 'use', kinds: ['importance'] },
    { section: 'origin', kinds: ['download', 'cmdrOperation'] },
  ])
}

// ── A big folder in a repo ──────────────────────────────────────────────────────

const SITE = `${HOME}/projects/website`
const siteSize = total(3_861_204_992, 'exact', 48_210, 6_114)
const siteHeadline: Headline = {
  what: { type: 'folder' },
  size: { bytes: siteSize.logical, atLeast: false },
  lead: { template: 'plain' },
}
const siteRemoval = derived(siteHeadline, [{ type: 'uncommittedWork' }], [], {
  shown: [{ type: 'showInFileManager' }, { type: 'moveToTrash' }],
  more: [],
})

const site: Stream = {
  plan: folderPlan(['git']),
  first: derived({ ...siteHeadline, size: null }, [{ type: 'cantTell' }]),
  facts: [
    {
      fact: digestFact({
        directChildren: 14,
        biggestChildren: [
          child(SITE, 'photos', 1_912_602_624, true),
          child(SITE, 'launch-video.mov', 980_418_560, false),
          child(SITE, 'node_modules', 641_728_512, true),
          child(SITE, '.git', 88_080_384, true),
          child(SITE, 'dist', 41_943_040, true),
          child(SITE, 'drafts', 18_874_368, true, 'lowerBound'),
          child(SITE, 'src', 12_582_912, true),
          child(SITE, 'pnpm-lock.yaml', 412_106, false),
        ],
        largestFiles: {
          state: 'known',
          value: [
            child(SITE, 'launch-video.mov', 980_418_560, false),
            child(`${SITE}/photos/2024`, 'team-offsite.mp4', 612_368_384, false),
            child(`${SITE}/photos/raw`, 'DSC_0412.NEF', 58_720_256, false),
            child(`${SITE}/photos/raw`, 'DSC_0413.NEF', 57_671_680, false),
            child(`${SITE}/node_modules/@esbuild/darwin-arm64/bin`, 'esbuild', 10_485_760, false),
            child(`${SITE}/.git/objects/pack`, 'pack-3f2a.pack', 8_388_608, false),
          ],
        },
        kindMix: [
          { kind: 'folder', count: 7 },
          { kind: 'document', count: 4 },
          { kind: 'code', count: 2 },
          { kind: 'video', count: 1 },
        ],
        unchangedForAYear: { state: 'known', value: { bytes: 1_586_495_488, of: siteSize.logical, atLeast: true } },
        class: 'plain',
      }),
      derived: siteRemoval,
    },
    { fact: identity('website', null, '2026-11-10'), derived: siteRemoval },
    { fact: sizeFact(siteSize), derived: siteRemoval },
    {
      fact: {
        kind: 'git',
        fact: {
          state: 'known',
          value: {
            repoRoot: SITE,
            branch: 'main',
            status: { state: 'known', value: { type: 'tracked', state: 'modified' } },
          },
        },
      },
      derived: siteRemoval,
    },
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
      derived: siteRemoval,
    },
    ...noRecords.map((fact) => ({ fact, derived: siteRemoval })),
  ],
  settled: true,
}

// ── node_modules: installed packages ────────────────────────────────────────────

const MODULES = `${SITE}/node_modules`
const modulesSize = total(641_728_512, 'exact', 31_480, 4_022)
const modulesHeadline: Headline = {
  what: { type: 'folder' },
  size: { bytes: modulesSize.logical, atLeast: false },
  lead: { template: 'plain' },
}
const modulesRemoval = derived(modulesHeadline, [{ type: 'rebuildableOutput', by: 'packageManager' }])

const nodeModules: Stream = {
  plan: folderPlan(['git']),
  first: derived({ ...modulesHeadline, size: null }, [{ type: 'cantTell' }]),
  facts: [
    {
      fact: digestFact({
        directChildren: 412,
        biggestChildren: [
          child(MODULES, '.pnpm', 588_251_136, true),
          child(MODULES, 'typescript', 23_068_672, true),
          child(MODULES, '@esbuild', 10_747_904, true),
          child(MODULES, '@sveltejs', 9_437_184, true),
          child(MODULES, 'vite', 3_145_728, true),
          child(MODULES, 'rollup', 2_621_440, true),
          child(MODULES, '.bin', 98_304, true),
          child(MODULES, '.modules.yaml', 41_984, false),
        ],
        largestFiles: {
          state: 'known',
          value: [
            child(`${MODULES}/@esbuild/darwin-arm64/bin`, 'esbuild', 10_485_760, false),
            child(`${MODULES}/.pnpm/typescript@5.9.3/node_modules/typescript/lib`, 'typescript.js', 9_437_184, false),
            child(
              `${MODULES}/.pnpm/@sveltejs+kit@2.48.0/node_modules/@sveltejs/kit/src`,
              'types.d.ts',
              2_097_152,
              false,
            ),
          ],
        },
        kindMix: [
          { kind: 'folder', count: 409 },
          { kind: 'document', count: 3 },
        ],
        unchangedForAYear: { state: 'known', value: { bytes: 76_546_048, of: modulesSize.logical, atLeast: true } },
        class: 'dependencies',
      }),
      derived: modulesRemoval,
    },
    { fact: identity('node_modules', null, '2026-11-08'), derived: modulesRemoval },
    { fact: sizeFact(modulesSize), derived: modulesRemoval },
    {
      fact: {
        kind: 'git',
        fact: { state: 'known', value: { repoRoot: SITE, branch: 'main', status: routine('notInStatus') } },
      },
      derived: modulesRemoval,
    },
    {
      fact: {
        kind: 'importance',
        fact: { state: 'known', value: { band: 'floored', stale: false, reasons: ['nameDenylisted'], visits: null } },
      },
      derived: modulesRemoval,
    },
    ...noRecords.map((fact) => ({ fact, derived: modulesRemoval })),
  ],
  settled: true,
}

// ── An empty folder ─────────────────────────────────────────────────────────────

const emptyHeadline: Headline = {
  what: { type: 'folder' },
  size: { bytes: 0, atLeast: false },
  lead: { template: 'plain' },
}
const emptyRemoval = derived(emptyHeadline, [{ type: 'userDocument' }])

const emptyFolder: Stream = {
  plan: folderPlan(),
  first: derived({ ...emptyHeadline, size: null }, [{ type: 'cantTell' }]),
  facts: [
    {
      fact: digestFact({
        directChildren: 0,
        biggestChildren: [],
        largestFiles: { state: 'known', value: [] },
        kindMix: [],
        unchangedForAYear: routine('noRecord'),
        class: 'plain',
      }),
      derived: emptyRemoval,
    },
    { fact: identity('New Folder', null, '2026-11-12'), derived: emptyRemoval },
    { fact: sizeFact(total(0, 'exact', 0, 0)), derived: emptyRemoval },
    {
      fact: {
        kind: 'importance',
        fact: { state: 'known', value: { band: 'low', stale: false, reasons: [], visits: null } },
      },
      derived: emptyRemoval,
    },
    ...noRecords.map((fact) => ({ fact, derived: emptyRemoval })),
  ],
  settled: true,
}

// ── A folder the index hasn't listed ────────────────────────────────────────────

const unlistedHeadline: Headline = { what: { type: 'folder' }, size: null, lead: { template: 'plain' } }
const unlistedRemoval = derived(unlistedHeadline, [{ type: 'userDocument' }])

const notIndexed: Stream = {
  plan: folderPlan(),
  first: derived(unlistedHeadline, [{ type: 'cantTell' }]),
  facts: [
    {
      fact: {
        kind: 'folderDigest',
        fact: {
          state: 'known',
          value: {
            class: 'plain',
            children: routine('notIndexed'),
            largestFiles: routine('notIndexed'),
            unchangedForAYear: routine('notIndexed'),
          },
        },
      },
      derived: unlistedRemoval,
    },
    { fact: identity('Scans 2019', null, '2019-04-02'), derived: unlistedRemoval },
    { fact: { kind: 'recursiveSize', fact: routine('notIndexed') }, derived: unlistedRemoval },
    { fact: { kind: 'importance', fact: routine('notIndexed') }, derived: unlistedRemoval },
    ...noRecords.map((fact) => ({ fact, derived: unlistedRemoval })),
  ],
  settled: true,
}

// ── An app, with its data in the Library ────────────────────────────────────────

const APP = 'Tessera.app'
const LIBRARY = `${HOME}/Library`
const appSize = total(412_090_368, 'exact', 2_310, 486)
const appData: AppData = {
  folders: [
    {
      name: 'example.tessera.mac',
      path: `${LIBRARY}/Containers/example.tessera.mac`,
      place: 'containers',
      size: { state: 'known', value: total(2_147_483_648, 'exact', 9_120, 380) },
    },
    {
      name: 'example.tessera.mac',
      path: `${LIBRARY}/Caches/example.tessera.mac`,
      place: 'caches',
      size: { state: 'known', value: total(318_767_104, 'lowerBound', 1_204, 31) },
    },
    {
      name: 'example.tessera.mac.savedState',
      path: `${LIBRARY}/Saved Application State/example.tessera.mac.savedState`,
      place: 'savedState',
      size: { state: 'known', value: total(65_536, 'exact', 3, 0) },
    },
  ],
  notLookedIn: ['groupContainers', 'logs'],
  totalBytes: 2_466_316_288,
  totalIsLowerBound: true,
}
const appHeadline: Headline = {
  what: { type: 'app', version: '2.4.1' },
  size: { bytes: appSize.logical, atLeast: false },
  lead: { template: 'lastOpened', at: day('2026-11-12') },
}
const appRemoval = derived(appHeadline, [{ type: 'removesApp', dataLeft: { state: 'known', value: appData } }], [], {
  shown: [{ type: 'showInFileManager' }, { type: 'moveToTrash' }],
  more: [],
})

const app: Stream = {
  plan: plan('appBundle', [
    { section: 'nature', kinds: ['identity', 'bundle', 'signature', 'recursiveSize'] },
    { section: 'context', kinds: ['appData'] },
    { section: 'use', kinds: ['lastOpened'] },
    { section: 'origin', kinds: ['download', 'cmdrOperation'] },
  ]),
  first: derived({ ...appHeadline, size: null, lead: { template: 'plain' } }, [
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
    { fact: sizeFact(appSize), derived: appRemoval },
    { fact: { kind: 'appData', fact: { state: 'known', value: appData } }, derived: appRemoval },
    {
      fact: { kind: 'lastOpened', fact: { state: 'known', value: { lastOpened: day('2026-11-12'), useCount: null } } },
      derived: appRemoval,
    },
    {
      fact: {
        kind: 'download',
        fact: {
          state: 'known',
          value: {
            sources: routine('noRecord'),
            downloadedAt: day('2026-03-02'),
            downloadedBy: 'Chrome',
            archive: null,
          },
        },
      },
      derived: appRemoval,
    },
    { fact: { kind: 'cmdrOperation', fact: routine('noRecord') }, derived: appRemoval },
  ],
  settled: true,
}

// ── ~/Downloads: a folder the system set up, not yet scored for importance ──────

const DOWNLOADS = `${HOME}/Downloads`
const downloadsSize = total(23_418_880_000, 'exact', 1_204, 96)
const downloadsHeadline: Headline = {
  what: { type: 'folder' },
  size: { bytes: downloadsSize.logical, atLeast: false },
  lead: { template: 'plain' },
}
const downloadsRemoval = derived(downloadsHeadline, [{ type: 'standardFolder', folder: 'downloads' }], [], {
  shown: [{ type: 'showInFileManager' }, { type: 'moveToTrash' }],
  more: [],
})

const downloads: Stream = {
  plan: folderPlan(['owner']),
  first: derived({ ...downloadsHeadline, size: null }, [{ type: 'cantTell' }]),
  facts: [
    {
      fact: digestFact({
        directChildren: 312,
        biggestChildren: [
          child(DOWNLOADS, 'Tessera-2.4.1-arm64.dmg', 412_318_720, false),
          child(DOWNLOADS, 'holiday-photos', 2_912_602_624, true),
          child(DOWNLOADS, 'Scans 2025.zip', 48_120_000, false),
        ],
        largestFiles: { state: 'known', value: [child(DOWNLOADS, 'Tessera-2.4.1-arm64.dmg', 412_318_720, false)] },
        kindMix: [
          { kind: 'document', count: 140 },
          { kind: 'image', count: 98 },
          { kind: 'archive', count: 41 },
          { kind: 'folder', count: 33 },
        ],
        unchangedForAYear: {
          state: 'known',
          value: { bytes: 9_120_000_000, of: downloadsSize.logical, atLeast: true },
        },
        class: 'plain',
      }),
      derived: downloadsRemoval,
    },
    { fact: identity('Downloads', null, '2026-11-13'), derived: downloadsRemoval },
    { fact: sizeFact(downloadsSize), derived: downloadsRemoval },
    {
      fact: { kind: 'owner', fact: { state: 'known', value: { type: 'standardFolder', folder: 'downloads' } } },
      derived: downloadsRemoval,
    },
    // The importance store hasn't scored it: its section collapses to the row's own words.
    { fact: { kind: 'importance', fact: routine('notIndexed') }, derived: downloadsRemoval },
    ...noRecords.map((fact) => ({ fact, derived: downloadsRemoval })),
  ],
  settled: true,
}

/** Keyed by the `tell-me-about-this` entry's state ids in `gallery-registry.ts`. */
export const aboutThisFolderFixtures: Record<string, AboutCardData> = {
  folder: card('website', SITE, site),
  'node-modules': card('node_modules', MODULES, nodeModules),
  'empty-folder': card('New Folder', `${HOME}/Desktop/New Folder`, emptyFolder),
  'not-indexed-folder': card('Scans 2019', '/Volumes/Archive/Scans 2019', notIndexed),
  app: card(APP, `/Applications/${APP}`, app),
  'standard-folder': card('Downloads', DOWNLOADS, downloads),
}
