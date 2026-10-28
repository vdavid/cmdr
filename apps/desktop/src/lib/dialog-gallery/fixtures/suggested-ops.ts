/**
 * Store patches for the AI suggestions dialog (`suggestedOpsState`).
 *
 * Each state is a BUILDER, not a literal: a patch carries live `SvelteMap` / `SvelteSet`
 * reviews the dialog mutates as the reviewer ticks boxes and shows more rows, so every opening
 * starts from a fresh copy. The reviews arrive with their windows and live facts already held,
 * so every row state renders without a backend, and nothing here is ever re-read
 * (`ensureLiveFacts` skips ops it already holds). Rows past the held window (the 60,000-file
 * group) page in through the patch's `rowSource`, answered from the same fixture.
 *
 * Raw copy on purpose: dev-only, outside the i18n-enforced areas.
 */

import type { SuggestedGroupView, SuggestedOpView, SuggestedSweepView } from '#lib/tauri-commands/index.js'
import {
  newGroupReview,
  suggestedOpsState,
  type GroupReview,
  type LiveFact,
  type SuggestedOpRowSource,
} from '#lib/suggested-ops/suggested-ops-trigger.svelte.js'
import { SvelteMap } from 'svelte/reactivity'
import { daysAgo, hoursAgo } from './relative-time'

type Patch = Partial<typeof suggestedOpsState>

const DOWNLOADS = '/Users/sam/Downloads'

/** One fixture file: its name, and what the disk says about it now (`present` by default). */
interface FixtureFile {
  name: string
  folder?: string
  size: number | null
  /** Unix seconds when suggested. */
  modified: number | null
  newName?: string
  /** What the live check found. Omitted: present and unchanged. */
  live?: LiveFact
  /** An earlier review turned it off (its approval then refused to start), so it opens unticked. */
  excluded?: boolean
}

interface FixtureGroup {
  groupId: number
  verb: SuggestedGroupView['verb']
  displayName: string
  rationale?: string
  sourceFolder?: string | null
  destination?: string | null
  destinationVolumeName?: string | null
  /** The destination's volume id: 'root' for this Mac. A NAS share stays `null`, as a saved share nobody
   *  connected arrives, so its pill shows the name and the generic icon. */
  destinationVolumeId?: string | null
  reversible?: SuggestedGroupView['reversible']
  runBlocker?: SuggestedGroupView['runBlocker']
  destinationState?: SuggestedGroupView['destinationState']
  fromSelector?: boolean
  files: FixtureFile[]
  /** The group's real size when it's bigger than the files listed (the 60,000-op case). */
  total?: number
  /** Makes up file `index` past `files`, for a group of `total` that pages as you scroll. */
  moreFiles?: (index: number) => FixtureFile
  /** A server card: no files, and this is what the click saves. */
  server?: SuggestedGroupView['server']
}

function view(group: FixtureGroup, sweepId: number): SuggestedGroupView {
  const total = group.total ?? group.files.length
  return {
    groupId: group.groupId,
    sweepId,
    verb: group.verb,
    status: 'pending',
    displayName: group.displayName,
    rationale: group.rationale ?? null,
    sourceVolumeId: 'root',
    sourceVolumeName: 'Macintosh HD',
    revision: 1,
    sourceFolder: group.sourceFolder === undefined ? DOWNLOADS : group.sourceFolder,
    destination: group.destination ?? null,
    destinationVolumeName: group.destinationVolumeName ?? null,
    destinationVolumeId: group.destinationVolumeId ?? null,
    reversible: group.reversible ?? 'restoreMove',
    runBlocker: group.runBlocker ?? null,
    destinationState: group.destinationState ?? (group.destination ? 'exists' : 'notApplicable'),
    liveOpCount: total - group.files.filter((file) => file.excluded).length,
    totalOpCount: total,
    fromSelector: group.fromSelector ?? false,
    server: group.server ?? null,
  }
}

/** The op at `index` of a group: a listed file, or one `moreFiles` makes up past them. */
function opOf(group: FixtureGroup, index: number): SuggestedOpView {
  const file = fileOf(group, index)
  return {
    opId: index + 1,
    sourcePath: `${file.folder ?? group.sourceFolder ?? DOWNLOADS}/${file.name}`,
    newName: file.newName ?? null,
    status: file.excluded ? 'excluded' : 'pending',
    snapshotSize: file.size,
    snapshotModified: file.modified,
  }
}

function fileOf(group: FixtureGroup, index: number): FixtureFile {
  return group.files[index] ?? group.moreFiles?.(index) ?? group.files[index % group.files.length]
}

function liveOf(file: FixtureFile): LiveFact {
  return file.live ?? { kind: 'present', size: file.size, modified: file.modified ?? hoursAgo(1) }
}

/**
 * Answers the trigger's row reads for a patch's groups (`suggestedOpsState.rowSource`), as the
 * store would: a group bigger than the rows its review holds keeps paging as you scroll. The
 * store itself has no fixture group and would answer a total of 0, emptying the list.
 */
function rowSourceFor(groups: FixtureGroup[]): SuggestedOpRowSource {
  const byId = new Map(groups.map((group) => [group.groupId, group]))
  return {
    page: ({ groupId, offset, limit }) => {
      const group = byId.get(groupId)
      if (!group || group.server) return Promise.resolve({ ops: [], offset, total: 0 })
      const total = group.total ?? group.files.length
      const end = Math.min(total, offset + limit)
      const ops = Array.from({ length: Math.max(0, end - offset) }, (_, i) => opOf(group, offset + i))
      return Promise.resolve({ ops, offset, total })
    },
    liveFacts: ({ groupId, offset, limit }) => {
      const group = byId.get(groupId)
      if (!group) return Promise.resolve({ kind: 'refused', reason: { type: 'unknownGroup' } })
      const facts = Array.from({ length: limit }, (_, i) => ({
        opId: offset + i + 1,
        state: liveOf(fileOf(group, offset + i)),
        newName: null,
      }))
      return Promise.resolve({ kind: 'facts', facts })
    },
  }
}

function review(group: FixtureGroup): GroupReview {
  const fresh = newGroupReview()
  fresh.seenRevision = 1
  // A server card has no files, so no window: the trigger never pages one either.
  if (group.server) return fresh
  const ops: SuggestedOpView[] = group.files.map((_, index) => opOf(group, index))
  fresh.window = { offset: 0, ops, total: group.total ?? ops.length }
  group.files.forEach((file, index) => {
    const live = liveOf(file)
    fresh.liveFacts.set(index + 1, live)
  })
  return fresh
}

function patch(
  sweeps: Array<{ sweepId: number; createdAt: number; rationale?: string; groups: FixtureGroup[] }>,
): Patch {
  const reviews = new SvelteMap<number, GroupReview>()
  const views: SuggestedSweepView[] = sweeps.map((sweep) => ({
    sweepId: sweep.sweepId,
    createdAt: sweep.createdAt,
    rationale: sweep.rationale ?? null,
    groups: sweep.groups.map((group) => {
      reviews.set(group.groupId, review(group))
      return view(group, sweep.sweepId)
    }),
  }))
  const rowSource = rowSourceFor(sweeps.flatMap((sweep) => sweep.groups))
  return { open: true, loading: false, loadError: false, sweeps: views, reviews, rowSource }
}

const MB = 1_000_000

/** The QA case the redesign came from: a month of Downloads, cleaned up to a NAS and the Trash. */
const DOWNLOADS_CLEANUP: FixtureGroup[] = [
  {
    groupId: 101,
    verb: 'move',
    displayName: 'Installers you’ve already run',
    rationale: 'These disk images installed apps that are now in Applications, so you don’t need them here.',
    destination: '/backups/Installers',
    destinationVolumeName: 'Naspolya',
    files: [
      { name: 'Docker.dmg', size: 612 * MB, modified: daysAgo(12) },
      { name: 'Figma-124.5.dmg', size: 291 * MB, modified: daysAgo(20), live: { kind: 'missing' } },
      {
        name: 'Obsidian-1.9.12-universal.dmg',
        size: 181 * MB,
        modified: daysAgo(25),
        live: { kind: 'present', size: 183 * MB, modified: hoursAgo(3) },
      },
      { name: 'Zoom.pkg', size: 74 * MB, modified: daysAgo(28), live: { kind: 'unknown' } },
    ],
  },
  {
    groupId: 102,
    verb: 'move',
    displayName: 'Invoices from 2026',
    rationale:
      'They all look like invoices, and you keep the others in your finances folder, named by date and sender.',
    destination: '/papers/finances/2026/invoices',
    destinationVolumeName: 'Naspolya',
    destinationState: 'willBeCreated',
    // A move that files under new names: the renamed rows draw an arrow, the rest keep theirs.
    files: [
      { name: 'invoice-2026-07.pdf', newName: '2026-07-31 Telia invoice.pdf', size: 184_320, modified: daysAgo(80) },
      { name: 'invoice-2026-08.pdf', newName: '2026-08-31 Telia invoice.pdf', size: 190_112, modified: daysAgo(52) },
      {
        name: 'invoice-2026-09.pdf',
        newName: '2026-09-27 OpenAI invoice Rymd.pdf',
        size: 176_004,
        modified: daysAgo(22),
      },
      { name: 'Számla_Rymdskottkärra_AB_2026_10_végleges.pdf', size: 233_871, modified: daysAgo(3) },
      // Nothing recorded when suggested, so a live value has nothing to have changed from.
      {
        name: 'receipt-ica.pdf',
        size: null,
        modified: null,
        live: { kind: 'present', size: 41_200, modified: daysAgo(2) },
      },
    ],
  },
  {
    groupId: 103,
    verb: 'move',
    displayName: 'Screenshot*.png',
    rationale: 'Screenshots pile up here. Your Pictures folder already has a Screenshots folder.',
    destination: '/photos/screenshots',
    destinationVolumeName: 'Naspolya',
    destinationState: 'unknown',
    fromSelector: true,
    files: Array.from({ length: 14 }, (_, i) => ({
      name: `Screenshot 2026-10-${String(10 + i)} at 11.0${String(i % 10)}.18.png`,
      size: 300_000 + i * 21_000,
      modified: daysAgo(16 - i),
    })),
  },
  {
    groupId: 104,
    verb: 'trash',
    displayName: 'Duplicate downloads',
    rationale: 'Each of these has a copy without the “(1)” in the same folder.',
    files: [
      { name: 'invoice-2026-08 (1).pdf', size: 190_112, modified: daysAgo(52) },
      { name: 'Docker (1).dmg', size: 612 * MB, modified: daysAgo(12) },
      { name: 'IMG_4417 (1).jpeg', size: 3_400_000, modified: daysAgo(6) },
    ],
  },
  {
    groupId: 105,
    verb: 'trash',
    displayName: 'Old exports on the NAS',
    rationale: 'Nobody opened these in over a year.',
    sourceFolder: '/exports/2024',
    runBlocker: 'noTrash',
    files: [
      { name: 'board-deck-2024-q1.key', size: 48 * MB, modified: daysAgo(560) },
      { name: 'board-deck-2024-q2.key', size: 51 * MB, modified: daysAgo(470) },
    ],
  },
  {
    groupId: 106,
    verb: 'rename',
    displayName: 'Scans with dates in their names',
    rationale: 'The scanner names files by number. The dates come from what each page says.',
    destination: DOWNLOADS,
    files: [
      { name: 'scan0007.pdf', newName: 'Scan 2026-05-02.pdf', size: 1_200_000, modified: daysAgo(40) },
      { name: 'scan0008.pdf', newName: 'Scan 2026-05-09.pdf', size: 980_000, modified: daysAgo(33) },
    ],
  },
  {
    groupId: 107,
    verb: 'copy',
    displayName: 'Tax papers to Dropbox',
    rationale: 'You asked to keep a copy of anything tax-related in Dropbox.',
    sourceFolder: null,
    destination: '/Users/sam/Dropbox/Taxes/2026',
    destinationVolumeName: 'Macintosh HD',
    destinationVolumeId: 'root',
    reversible: 'deleteWhatWasWritten',
    files: [
      { name: 'K4-2026.pdf', folder: DOWNLOADS, size: 88_000, modified: daysAgo(9) },
      { name: 'tax-summary.xlsx', folder: '/Users/sam/Documents/Money', size: 42_310, modified: daysAgo(30) },
    ],
  },
]

/** Raw file `index` of the 60,000-file group: a year of shots, oldest first. */
function rawCameraFile(index: number): FixtureFile {
  return {
    name: `DSC${String(10_000 + index).padStart(5, '0')}.ARW`,
    size: 24 * MB + (index % 1_000) * 1_000,
    modified: daysAgo(365 - (index * 365) / 60_000),
  }
}

/** The ones that can't be taken back, and a group of 60,000 that must still open instantly. */
const HARD_CASES: FixtureGroup[] = [
  {
    groupId: 201,
    verb: 'delete',
    displayName: 'Build caches',
    rationale: 'These are regenerated on the next build.',
    sourceFolder: '/Users/sam/projects/cmdr/target',
    reversible: 'irreversible',
    files: [
      { name: 'debug', size: null, modified: daysAgo(1) },
      { name: 'release', size: null, modified: daysAgo(3) },
    ],
  },
  {
    groupId: 202,
    verb: 'compress',
    displayName: 'Archive last year’s photos',
    rationale: 'You don’t open these any more, and they take a lot of space.',
    sourceFolder: '/Users/sam/Pictures/2025',
    destination: '/Users/sam/Pictures/2025.zip',
    destinationVolumeName: 'Macintosh HD',
    destinationVolumeId: 'root',
    reversible: 'irreversible',
    files: [{ name: 'Summer', size: null, modified: daysAgo(300) }],
  },
  {
    groupId: 203,
    verb: 'move',
    displayName: 'Raw camera files',
    rationale: 'Your camera folder has 60,000 raw files you already imported.',
    sourceFolder: '/Volumes/SD/DCIM',
    destination: '/media/raw',
    destinationVolumeName: 'Naspolya',
    total: 60_000,
    // The first page is held, as a real review's would be; the rest page in through `rowSource`.
    files: Array.from({ length: 200 }, (_, i) => rawCameraFile(i)),
    moreFiles: rawCameraFile,
  },
]

/** Server cards: an SFTP add at an address Cmdr has never saved, and a NAS host's edit. */
const SERVER_CARDS: FixtureGroup[] = [
  {
    groupId: 301,
    verb: 'addServer',
    displayName: 'Build box',
    rationale: 'You open sftp://ada@build.example.com in the terminal every day, and it isn’t in your servers yet.',
    reversible: 'serverSettings',
    sourceFolder: null,
    files: [],
    server: {
      protocol: 'sftp',
      fields: [
        { field: 'name', value: 'Build box', changed: false, previous: null },
        { field: 'address', value: 'build.example.com', changed: false, previous: null },
        { field: 'port', value: '22', changed: false, previous: null },
        { field: 'account', value: 'ada', changed: false, previous: null },
        { field: 'folder', value: '/srv/builds', changed: false, previous: null },
      ],
      addressIsNew: true,
      sharesAffected: null,
    },
  },
  {
    groupId: 302,
    verb: 'editServer',
    displayName: 'Naspolya',
    rationale: 'Your NAS shows as “naspi.local” in Servers. You call it Naspolya everywhere else.',
    reversible: 'serverSettings',
    sourceFolder: null,
    files: [],
    server: {
      protocol: 'smb',
      fields: [
        { field: 'name', value: 'Naspolya', changed: true, previous: 'naspi.local' },
        { field: 'address', value: 'naspi.local', changed: false, previous: null },
        { field: 'account', value: 'sam', changed: true, previous: null },
      ],
      addressIsNew: false,
      sharesAffected: 3,
    },
  },
]

export const suggestedOpsFixtures: Record<string, () => Patch> = {
  'downloads-cleanup': () =>
    patch([
      {
        sweepId: 1,
        createdAt: hoursAgo(2),
        rationale:
          'Your Downloads folder has 30 files from the last month. Installers and paperwork can go to the NAS, and the duplicates can go.',
        groups: DOWNLOADS_CLEANUP,
      },
    ]),
  'hard-cases': () => patch([{ sweepId: 2, createdAt: daysAgo(3), groups: HARD_CASES }]),
  servers: () => patch([{ sweepId: 3, createdAt: hoursAgo(1), groups: SERVER_CARDS }]),
  'server-refused': () => ({
    ...patch([{ sweepId: 3, createdAt: hoursAgo(1), groups: SERVER_CARDS }]),
    refusals: new SvelteMap([
      [301, { kind: 'server', answer: { kind: 'nameTaken', name: 'Build box' } }],
      [302, { kind: 'server', answer: { kind: 'serverChanged' } }],
    ]),
  }),
  'changed-and-refused': () => {
    // The refused group's receipt was unticked before that approval, so it reopens unticked.
    const [installers, invoices] = DOWNLOADS_CLEANUP
    const refused = {
      ...invoices,
      files: invoices.files.map((file) => (file.name === 'receipt-ica.pdf' ? { ...file, excluded: true } : file)),
    }
    const seeded = patch([{ sweepId: 1, createdAt: hoursAgo(2), groups: [installers, refused] }])
    const changed = seeded.reviews?.get(101)
    if (changed) changed.changedUnderReview = true
    return {
      ...seeded,
      decisionNotice: 'suggestedOps.approvalUnsure',
      refusals: new SvelteMap([[102, { kind: 'nothingToRun' }]]),
    }
  },
  empty: () => ({ open: true, loading: false, loadError: false, sweeps: [], reviews: new SvelteMap() }),
  'load-error': () => ({ open: true, loading: false, loadError: true, sweeps: [], reviews: new SvelteMap() }),
}
