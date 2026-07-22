/**
 * Fixtures for the store-seeded dialogs, plus the binding from each dialog to
 * the store its patch lands on.
 *
 * These five take no content props: they read a module-level `$state` store and
 * the APP mounts them (`+page.svelte` for the rename review, "What's new", and
 * the operation log; `+layout.svelte` for feedback and error reports). So a
 * fixture here is a PATCH, not a prop bag, and applying it runs the app's real
 * render path. `store-seeding.ts` derives the undo from the patch's own keys, so
 * nothing below has to describe how to clean up after itself.
 *
 * Raw copy on purpose: this module is dev-only and sits outside the
 * i18n-enforced areas, so fixture strings never reach the message catalog.
 */

import { askCmdrState } from '$lib/ask-cmdr/ask-cmdr-trigger.svelte'
import { errorReportFlow } from '$lib/error-reporter/error-report-flow.svelte'
import { feedbackFlow } from '$lib/feedback/feedback-flow.svelte'
import { operationLogState } from '$lib/operation-log/operation-log-trigger.svelte'
import { whatsNewState } from '$lib/whats-new/whats-new-trigger.svelte'
import type { OperationRow } from '$lib/tauri-commands'
import { storeSeed, type StoreSeed } from '../store-seeding'
import { daysAgo, hoursAgo } from './relative-time'

/** The dialogs the gallery opens by seeding a store instead of passing props. */
export type StoreSeededDialogId = 'bulk-rename-review' | 'error-report' | 'feedback' | 'operation-log' | 'whats-new'

type Patch<T> = Partial<T> | undefined

// ── Bulk rename review (`askCmdrState.renameReview`) ────────────────────────

/**
 * Builds a review the way `openRenameReview` does: a proposal id, display rows,
 * and the preflight flags. The id is a fixture one, so Apply fails against the
 * backend (see the gallery row's note) — the review itself renders exactly as a
 * real proposal does.
 */
function review(
  rows: Array<{
    sourceName: string
    destinationName: string
    allowed?: boolean
    blockedReason?: 'targetExists' | 'sourceMissing' | null
    warnings?: Array<'extensionChanged' | 'cycle'>
  }>,
  extra: { expired?: boolean; preflighting?: boolean } = {},
) {
  return {
    proposalId: 'gallery-fixture-proposal',
    rows: rows.map((row, index) => ({
      rowId: `gallery-row-${String(index)}`,
      sourceName: row.sourceName,
      destinationName: row.destinationName,
      allowed: row.allowed ?? row.blockedReason == null,
      blockedReason: row.blockedReason ?? null,
      warnings: row.warnings ?? [],
    })),
    preflighting: extra.preflighting ?? false,
    expired: extra.expired ?? false,
    requestVersion: 0,
  }
}

/**
 * Names short enough to render in full at the dialog's default width. The point
 * of this state is a calm, all-allowed list where the six rows read as six
 * different renames; the long-name stress lives in its own state, where the
 * middle-ellipsis is the thing being reviewed.
 */
const TIDY_ROWS = [
  { sourceName: 'DSC09241.arw', destinationName: 'Sunrise 01.arw' },
  { sourceName: 'DSC09242.arw', destinationName: 'Sunrise 02.arw' },
  { sourceName: 'DSC09243.arw', destinationName: 'Sunrise 03.arw' },
  { sourceName: 'DSC09244.arw', destinationName: 'Sunrise 04.arw' },
  { sourceName: 'DSC09245.arw', destinationName: 'Sunrise 05.arw' },
  { sourceName: 'DSC09246.arw', destinationName: 'Sunrise 06.arw' },
]

export const bulkRenameFixtures: Record<string, Patch<typeof askCmdrState>> = {
  'all-allowed': { renameReview: review(TIDY_ROWS) },
  // Every badge and both blocked reasons at once: the row states are what this
  // dialog is FOR, and they never co-occur in a tidy proposal.
  'some-blocked': {
    renameReview: review([
      { sourceName: 'invoice-2026-06.pdf', destinationName: 'Invoice 2026-06.pdf' },
      { sourceName: 'invoice-2026-07.pdf', destinationName: 'Invoice 2026-07.pdf', blockedReason: 'targetExists' },
      { sourceName: 'invoice-2026-08.pdf', destinationName: 'Invoice 2026-08.pdf', blockedReason: 'sourceMissing' },
      {
        sourceName: 'scan-2026-05.jpeg',
        destinationName: 'Invoice 2026-05.pdf',
        warnings: ['extensionChanged'],
      },
      { sourceName: 'Invoice 2026-04.pdf', destinationName: 'invoice-2026-04.pdf', warnings: ['cycle'] },
      { sourceName: 'notes.txt', destinationName: 'Notes.txt', allowed: false },
    ]),
  },
  // Middle-ellipsis territory: names past the column width, non-ASCII, and a
  // name that only differs from its neighbour deep inside the string.
  'long-names': {
    renameReview: review([
      {
        sourceName: '2026-07-14_stockholm-archipelago-sunrise-session_DSC09241_edited_final_v3_reallyfinal.arw',
        destinationName: '2026-07-14 Stockholm archipelago sunrise session — frame 01 (edited, final).arw',
      },
      {
        sourceName: '2026-07-14_stockholm-archipelago-sunrise-session_DSC09242_edited_final_v3_reallyfinal.arw',
        destinationName: '2026-07-14 Stockholm archipelago sunrise session — frame 02 (edited, final).arw',
      },
      {
        sourceName: 'Számla_Rymdskottkärra_AB_2026_07_végleges_javított_ÁFA-val.pdf',
        destinationName: 'Rymdskottkärra AB — számla 2026-07 (ÁFA-val, végleges).pdf',
        warnings: ['extensionChanged'],
      },
      {
        sourceName: 'この写真のファイル名はとても長いです_2026年7月_ストックホルム_夕日.jpg',
        destinationName: 'ストックホルムの夕日 2026-07-14 — 01.jpg',
      },
    ]),
  },
  // The proposal outlived its backend staging: the table is replaced by a notice
  // and Apply is dead. Unreachable in practice without waiting one out.
  expired: { renameReview: review(TIDY_ROWS, { expired: true }) },
}

// ── Feedback (`feedbackFlow`) ───────────────────────────────────────────────

export const feedbackFixtures: Record<string, Patch<typeof feedbackFlow>> = {
  default: { open: true },
}

// ── Error report (`errorReportFlow`) ────────────────────────────────────────

export const errorReportFixtures: Record<string, Patch<typeof errorReportFlow>> = {
  blank: { open: true, initialNote: '' },
  // What the toast's "Send error report…" link ferries in: a multi-line message
  // with a path in it, which is where the note box's sizing shows its hand.
  'from-toast': {
    open: true,
    initialNote:
      'Couldn’t copy “2026-07-14_stockholm-archipelago-sunrise-session_DSC09241_edited_final_v3.arw”.\nThe destination volume disconnected partway through.\nPath: /Volumes/Naspolya/media/photos/2026/07-summer-archive/raw-originals/Sony-A7RV',
  },
}

// ── What's new (`whatsNewState`) ────────────────────────────────────────────

export const whatsNewFixtures: Record<string, Patch<typeof whatsNewState>> = {
  'one-release': {
    open: true,
    allowEmpty: false,
    releases: [
      {
        version: '0.31.0',
        date: '2026-07-20',
        lead: '**Ask Cmdr can rename in bulk now.** Describe the naming you want and review every row before anything touches disk.',
        sections: [
          {
            title: 'Added',
            entries: [
              'Bulk rename review: allow or deny each row, with warnings for extension changes and rename cycles.',
              'The operation log records who started an operation: you, an AI client, or the agent.',
            ],
          },
          {
            title: 'Fixed',
            entries: ['Copying to a disconnected network share now explains itself instead of stalling.'],
          },
        ],
      },
    ],
  },
  // The realistic post-update case: several releases, long entries, and a lead
  // that's a numbered list (block markdown, which is why the lead is a <div>).
  'several-releases': {
    open: true,
    allowEmpty: false,
    releases: [
      {
        version: '0.31.0',
        date: '2026-07-20',
        lead: '**Two big ones this time:**\n\n1. Ask Cmdr can rename in bulk, with a review step.\n2. The file viewer opens 4 GB logs without breaking a sweat.',
        sections: [
          {
            title: 'Added',
            entries: [
              'Bulk rename review: allow or deny each row, with warnings for extension changes and rename cycles.',
              'The file viewer streams big files instead of loading them, so a 4 GB log opens as fast as a 4 KB one.',
              'Volume tints: give each drive a colour so you always know which pane you’re in.',
            ],
          },
          {
            title: 'Changed',
            entries: [
              'The transfer dialog shows real throughput and an honest ETA, and both keep updating while the scan is still running.',
            ],
          },
          { title: 'Fixed', entries: ['Copying to a disconnected network share explains itself instead of stalling.'] },
        ],
      },
      {
        version: '0.30.2',
        date: '2026-07-11',
        lead: null,
        sections: [
          {
            title: 'Fixed',
            entries: [
              'MTP devices reconnect after sleep instead of showing an empty pane.',
              'The search index no longer re-scans an external drive that never went stale.',
            ],
          },
          {
            title: 'Security',
            entries: ['Archive extraction rejects entries that would escape the destination folder.'],
          },
        ],
      },
      {
        version: '0.30.1',
        date: '2026-07-03',
        lead: 'A quiet one: mostly indexing throughput.',
        sections: [
          { title: 'Changed', entries: ['Indexing a big drive uses about a third of the memory it used to.'] },
        ],
      },
    ],
  },
  // Reachable only through the manual Help reopen: an auto-show with nothing to
  // say collapses to a silent stamp instead of an empty popup.
  empty: { open: true, allowEmpty: true, releases: [] },
}

// ── Operation log (`operationLogState`) ─────────────────────────────────────

/** One log row, with the fields the dialog doesn't display filled in plausibly. */
function operation(row: Partial<OperationRow> & Pick<OperationRow, 'opId' | 'kind' | 'itemCount'>): OperationRow {
  return {
    archiveSubkind: null,
    initiator: 'user',
    executionStatus: 'done',
    rollbackState: 'rollbackable',
    notRollbackableReason: null,
    rollsBackOpId: null,
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    startedAt: hoursAgo(1),
    endedAt: hoursAgo(1),
    itemsDone: row.itemCount,
    bytesTotal: 1_048_576,
    searchCoverage: 'full',
    searchCoverageReason: null,
    devSummary: null,
    ...row,
  }
}

const LOGGED_OPERATIONS: OperationRow[] = [
  operation({
    opId: 'gallery-op-1',
    kind: 'copy',
    itemCount: 1_284,
    bytesTotal: 48_318_382_080,
    startedAt: hoursAgo(1),
  }),
  operation({
    opId: 'gallery-op-2',
    kind: 'rename',
    itemCount: 96,
    initiator: 'agent',
    startedAt: hoursAgo(3),
    endedAt: hoursAgo(3),
  }),
  operation({
    opId: 'gallery-op-3',
    kind: 'move',
    itemCount: 12,
    executionStatus: 'failed',
    rollbackState: 'partiallyRolledBack',
    startedAt: hoursAgo(9),
    endedAt: hoursAgo(9),
  }),
  operation({
    opId: 'gallery-op-4',
    kind: 'trash',
    itemCount: 3,
    rollbackState: 'rolledBack',
    startedAt: daysAgo(1),
    endedAt: daysAgo(1),
  }),
  operation({
    opId: 'gallery-op-5',
    kind: 'archiveEdit',
    archiveSubkind: 'compress',
    itemCount: 41,
    initiator: 'aiClient',
    startedAt: daysAgo(2),
    endedAt: daysAgo(2),
  }),
  operation({
    opId: 'gallery-op-6',
    kind: 'delete',
    itemCount: 1,
    executionStatus: 'canceled',
    rollbackState: 'notRollbackable',
    notRollbackableReason: 'permanentDelete',
    startedAt: daysAgo(4),
    endedAt: daysAgo(4),
  }),
  operation({ opId: 'gallery-op-7', kind: 'createFolder', itemCount: 1, startedAt: daysAgo(9), endedAt: daysAgo(9) }),
]

export const operationLogFixtures: Record<string, Patch<typeof operationLogState>> = {
  loading: { open: true, loading: true, entries: [], loadError: false, hasMore: false },
  populated: { open: true, loading: false, entries: LOGGED_OPERATIONS, loadError: false, hasMore: false },
  'more-pages': { open: true, loading: false, entries: LOGGED_OPERATIONS, loadError: false, hasMore: true },
  empty: { open: true, loading: false, entries: [], loadError: false, hasMore: false },
  'load-error': { open: true, loading: false, entries: [], loadError: true, hasMore: false },
}

// ── The store binding ───────────────────────────────────────────────────────

function seedFrom<T extends object>(store: T, patch: Partial<T> | undefined, isOpen: () => boolean): StoreSeed | null {
  return patch === undefined ? null : storeSeed(store, patch, isOpen)
}

/**
 * Resolves a request to the seed that opens it, or `null` when the state id has
 * no fixture. Each case names the store, its fixture record, and how the app
 * itself decides the dialog is showing — the three facts the harness needs to
 * seed, watch, and restore.
 */
export function buildStoreSeed(dialogId: StoreSeededDialogId, stateId: string): StoreSeed | null {
  switch (dialogId) {
    case 'bulk-rename-review':
      return seedFrom(askCmdrState, bulkRenameFixtures[stateId], () => askCmdrState.renameReview !== null)
    case 'error-report':
      return seedFrom(errorReportFlow, errorReportFixtures[stateId], () => errorReportFlow.open)
    case 'feedback':
      return seedFrom(feedbackFlow, feedbackFixtures[stateId], () => feedbackFlow.open)
    case 'operation-log':
      return seedFrom(operationLogState, operationLogFixtures[stateId], () => operationLogState.open)
    case 'whats-new':
      return seedFrom(whatsNewState, whatsNewFixtures[stateId], () => whatsNewState.open)
  }
}
