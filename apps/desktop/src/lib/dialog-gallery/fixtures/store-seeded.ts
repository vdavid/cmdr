/**
 * Fixtures for the store-seeded dialogs, plus the binding from each dialog to
 * the store its patch lands on.
 *
 * These six take no content props: they read a module-level `$state` store and
 * the APP mounts them (`+page.svelte` for the rename review, "What's new", the
 * operation log, and AI suggestions, whose patches live in `./suggested-ops`;
 * `+layout.svelte` for feedback and error reports). So a fixture here is a
 * PATCH, not a prop bag, and applying it runs the app's real
 * render path. `store-seeding.ts` derives the undo from the patch's own keys, so
 * nothing below has to describe how to clean up after itself.
 *
 * Raw copy on purpose: this module is dev-only and sits outside the
 * i18n-enforced areas, so fixture strings never reach the message catalog.
 */

import { askCmdrState } from '#lib/ask-cmdr/ask-cmdr-trigger.svelte.js'
import { errorReportFlow } from '#lib/error-reporter/error-report-flow.svelte.js'
import { feedbackFlow } from '#lib/feedback/feedback-flow.svelte.js'
import { operationLogState } from '#lib/operation-log/operation-log-trigger.svelte.js'
import { whatsNewState } from '#lib/whats-new/whats-new-trigger.svelte.js'
import { suggestedOpsState } from '#lib/suggested-ops/suggested-ops-trigger.svelte.js'
import type { OperationRow, RenameEvidence, RenameEvidenceCoverage } from '#lib/tauri-commands/index.js'
import { storeSeed, type StoreSeed } from '../store-seeding'
import { daysAgo, hoursAgo } from './relative-time'
import { suggestedOpsFixtures } from './suggested-ops'

/** The dialogs the gallery opens by seeding a store instead of passing props. */
export type StoreSeededDialogId =
  | 'bulk-rename-review'
  | 'error-report'
  | 'feedback'
  | 'operation-log'
  | 'suggested-ops'
  | 'whats-new'

type Patch<T> = Partial<T> | undefined

// ── Bulk rename review (`askCmdrState.renameReview`) ────────────────────────

interface GalleryRenameRow {
  sourceName: string
  destinationName: string
  evidence?: RenameEvidence
  coverage?: RenameEvidenceCoverage
  allowed?: boolean
  blockedReason?: 'targetExists' | 'sourceMissing' | null
  warnings?: Array<'extensionChanged' | 'cycle'>
  nameRejected?: boolean
  /** The folder this row's file lives in, for the states that span more than one. */
  folder?: string
}

/**
 * Builds one staged batch the way the end of a turn does: a proposal id, display rows, and the
 * preflight flags. The id is a fixture one, so Apply fails against the backend (see the gallery
 * row's note) — the batch itself renders exactly as a real proposal does.
 */
function batch(
  rows: GalleryRenameRow[],
  extra: {
    expired?: boolean
    preflighting?: boolean
    id?: string
    checkFailed?: boolean
    checkProgress?: { checked: number; total: number }
  } = {},
) {
  return {
    proposalId: extra.id ?? 'gallery-fixture-proposal',
    rows: rows.map((row, index) => ({
      rowId: `${extra.id ?? 'gallery'}-row-${String(index)}`,
      sourceName: row.sourceName,
      destinationName: row.destinationName,
      // A fixture path that doesn't exist, so every row shows the no-thumbnail placeholder.
      // That's the degraded state on purpose: the gallery reviews layout, not real images.
      sourcePath: `${row.folder ?? '/gallery-fixture'}/${row.sourceName}`,
      volumeId: 'root',
      evidence: row.evidence ?? { source: 'filename' as const, detail: row.sourceName },
      coverage: row.coverage ?? null,
      allowed: row.allowed ?? row.blockedReason == null,
      blockedReason: row.blockedReason ?? null,
      warnings: row.warnings ?? [],
      nameRejected: row.nameRejected ?? false,
    })),
    preflighting: extra.preflighting ?? false,
    expired: extra.expired ?? false,
    revision: 1,
    changed: false,
    checkFailed: extra.checkFailed ?? false,
    checkProgress: extra.checkProgress ?? null,
    requestVersion: 0,
  }
}

/** One review over one batch: what a job small enough for a single model reply looks like. */
function review(rows: GalleryRenameRow[], extra: Parameters<typeof batch>[1] = {}) {
  return { proposals: [batch(rows, extra)], notice: null }
}

/** A raw-file batch has no recognized text, so every row names its honest limit. */
const SHOT_AT: RenameEvidence = { source: 'metadata', detail: 'Shot 2026-07-14, 05:12' }

/**
 * Names short enough to render in full at the dialog's default width. The point
 * of this state is a calm, all-allowed list where the six rows read as six
 * different renames; the long-name stress lives in its own state, where the
 * middle-ellipsis is the thing being reviewed.
 */
const TIDY_ROWS = [
  { sourceName: 'DSC09241.arw', destinationName: 'Sunrise 01.arw', evidence: SHOT_AT },
  { sourceName: 'DSC09242.arw', destinationName: 'Sunrise 02.arw', evidence: SHOT_AT },
  { sourceName: 'DSC09243.arw', destinationName: 'Sunrise 03.arw', evidence: SHOT_AT },
  { sourceName: 'DSC09244.arw', destinationName: 'Sunrise 04.arw', evidence: SHOT_AT },
  { sourceName: 'DSC09245.arw', destinationName: 'Sunrise 05.arw', evidence: SHOT_AT },
  { sourceName: 'DSC09246.arw', destinationName: 'Sunrise 06.arw', evidence: SHOT_AT },
]

export const bulkRenameFixtures: Record<string, Patch<typeof askCmdrState>> = {
  'all-allowed': { renameReview: review(TIDY_ROWS) },
  // Every badge and both blocked reasons at once: the row states are what this
  // dialog is FOR, and they never co-occur in a tidy proposal.
  'some-blocked': {
    renameReview: review([
      { sourceName: 'invoice-2026-06.pdf', destinationName: 'Invoice 2026-06.pdf' },
      {
        sourceName: 'invoice-2026-07.pdf',
        destinationName: 'Invoice 2026-07.pdf',
        blockedReason: 'targetExists',
      },
      {
        sourceName: 'invoice-2026-08.pdf',
        destinationName: 'Invoice 2026-08.pdf',
        blockedReason: 'sourceMissing',
      },
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
  // The evidence column is the point of this one: all five sources side by side, so the
  // honest "nothing was read inside the file" labels can be compared against a real quote —
  // and a decisive quote can be compared against a sliver of a page of recognized text.
  'why-this-name': {
    renameReview: review([
      {
        sourceName: 'Screenshot 2026-07-24 at 11.03.18.png',
        destinationName: 'LinkedIn inbox 2026-07-24.png',
        evidence: { source: 'imageText', detail: 'LinkedIn · Messaging · 3 new messages' },
        coverage: {
          matchOffset: 0,
          matchedChars: 37,
          deliveredChars: 96,
          contextBefore: '',
          matchedText: 'LinkedIn · Messaging · 3 new messages',
          contextAfter: ' · Jobs · My Network',
          trimmedBefore: false,
          trimmedAfter: false,
        },
      },
      {
        sourceName: 'IMG_4417.jpeg',
        destinationName: 'Sunset over Årstaviken.jpeg',
        evidence: { source: 'imageTags', detail: 'sunset, water, city skyline' },
      },
      {
        sourceName: 'scan0007.pdf',
        destinationName: 'Scan 2026-05-02.pdf',
        evidence: { source: 'metadata', detail: 'Created 2026-05-02, 1.2 MB' },
      },
      {
        sourceName: 'kvitto-ica.png',
        destinationName: 'ICA receipt.png',
        evidence: { source: 'filename', detail: 'kvitto-ica' },
      },
      {
        sourceName: 'DSC00812.arw',
        destinationName: '2026-07-14 - 01.arw',
        evidence: { source: 'userInstruction', detail: 'you asked for a YYYY-MM-DD prefix and a counter' },
      },
      {
        sourceName: 'Screenshot 2026-07-24 at 11.09.51.png',
        destinationName: 'Cargo build warnings.png',
        evidence: {
          source: 'imageText',
          detail:
            'warning: unused import: `EvidenceProblem` --> src/tools/propose/rename.rs:899 warning: `cmdr` generated 1 warning',
        },
        coverage: {
          matchOffset: 1_284,
          matchedChars: 121,
          deliveredChars: 3_140,
          contextBefore: 'Compiling cmdr v0.36.2 ',
          matchedText:
            'warning: unused import: `EvidenceProblem` --> src/tools/propose/rename.rs:899 warning: `cmdr` generated 1 warning',
          contextAfter: ' Finished dev profile',
          trimmedBefore: true,
          trimmedAfter: true,
        },
      },
      // A real quote that proves almost nothing: 14 characters of 3,140, against the row above
      // it whose quote covers 121 of the same 3,140. This pair is what the coverage hint is
      // for, and "invoice" for a payment confirmation is exactly the wrong name it can't catch.
      {
        sourceName: 'Screenshot 2026-07-24 at 11.12.07.png',
        destinationName: 'Klarna invoice 1 299 kr.png',
        evidence: { source: 'imageText', detail: 'Total 1 299 kr' },
        coverage: {
          matchOffset: 2_140,
          matchedChars: 14,
          deliveredChars: 3_140,
          contextBefore: 'Betalning mottagen  ',
          matchedText: 'Total 1 299 kr',
          contextAfter: '  Tack för ditt köp',
          trimmedBefore: true,
          trimmedAfter: true,
        },
      },
    ]),
  },
  // What the row states around EDITING look like side by side: a name the user typed (no
  // evidence at all), a name Cmdr kept because it read nothing inside the file, two rows whose
  // names rest on nothing read, and a refused name sitting on the row it couldn't replace.
  'edited-names': {
    renameReview: review([
      {
        sourceName: 'Screenshot 2026-07-24 at 11.12.07.png',
        destinationName: 'Klarna payment confirmation 2026-07-24.png',
        evidence: { source: 'userEdited', detail: '' },
      },
      {
        sourceName: 'IMG_4417.jpeg',
        destinationName: 'IMG_4417.jpeg',
        evidence: { source: 'metadata', detail: 'Shot 2026-07-14, 05:12' },
      },
      {
        sourceName: 'scan0007.pdf',
        destinationName: 'Scan 2026-05-02.pdf',
        evidence: { source: 'metadata', detail: 'Created 2026-05-02, 1.2 MB' },
      },
      {
        sourceName: 'DSC00812.arw',
        destinationName: '2026-07-14 - 01.arw',
        evidence: { source: 'userInstruction', detail: 'you asked for a YYYY-MM-DD prefix and a counter' },
        nameRejected: true,
      },
    ]),
  },
  // One job, two folders, two batches: the model can only emit about 101 plan rows per reply,
  // and a rename group binds one parent, so a job like this is several proposals and one
  // review. The folder headings are what keep a row's folder from being a guess.
  'spanning-folders': {
    renameReview: {
      notice: null,
      proposals: [
        batch(
          [
            {
              sourceName: 'DSC09241.arw',
              destinationName: 'Sunrise 01.arw',
              evidence: SHOT_AT,
              folder: '/Photos/2026-07 archipelago',
            },
            {
              sourceName: 'DSC09242.arw',
              destinationName: 'Sunrise 02.arw',
              evidence: SHOT_AT,
              folder: '/Photos/2026-07 archipelago',
            },
          ],
          { id: 'gallery-batch-one' },
        ),
        batch(
          [
            {
              sourceName: 'invoice-2026-06.pdf',
              destinationName: 'Invoice 2026-06.pdf',
              folder: '/Documents/Rymdskottkärra AB/invoices',
            },
            {
              sourceName: 'invoice-2026-07.pdf',
              destinationName: 'Invoice 2026-07.pdf',
              folder: '/Documents/Rymdskottkärra AB/invoices',
            },
            {
              sourceName: 'invoice-2026-08.pdf',
              destinationName: 'Invoice 2026-08.pdf',
              folder: '/Documents/Rymdskottkärra AB/invoices',
              blockedReason: 'targetExists',
            },
          ],
          { id: 'gallery-batch-two' },
        ),
      ],
    },
  },
  // The proposal outlived its backend staging: the table is replaced by a notice
  // and Apply is dead. Unreachable in practice without waiting one out.
  expired: { renameReview: review(TIDY_ROWS, { expired: true }) },
  // A slow check on a network drive: no time limit, a count, and a way to stop.
  checking: { renameReview: review(TIDY_ROWS, { preflighting: true, checkProgress: { checked: 2, total: 6 } }) },
  'check-unfinished': { renameReview: review(TIDY_ROWS, { checkFailed: true }) },
  // A refusal the volume explains: the catalog's own sentence for it sits under ours.
  'couldnt-start': {
    renameReview: {
      ...review(TIDY_ROWS),
      notice: {
        kind: 'couldntStart' as const,
        reason: {
          type: 'engine' as const,
          error: { type: 'source_not_connected' as const, path: '/gallery-fixture/DSC09241.arw' },
        },
      },
    },
  },
  'needs-another-look': { renameReview: { ...review(TIDY_ROWS), notice: { kind: 'needsAnotherLook' as const } } },
}

// ── Feedback (`feedbackFlow`) ───────────────────────────────────────────────

export const feedbackFixtures: Record<string, Patch<typeof feedbackFlow>> = {
  default: { open: true },
}

// ── Error report (`errorReportFlow`) ────────────────────────────────────────

export const errorReportFixtures: Record<string, Patch<typeof errorReportFlow>> = {
  blank: { open: true, initialNote: '' },
  // Amend mode, the state the "Error report sent" toast opens. It reads the REAL Flow B
  // stash, so what this row shows depends on whether this run auto-sent anything.
  amend: { open: true, initialNote: '', mode: 'amend' },
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
        leadHtml:
          '<p><strong>Ask Cmdr can rename in bulk now.</strong> Describe the naming you want and review every row before anything touches disk.</p>\n',
        sections: [
          {
            title: 'Added',
            entriesHtml: [
              'Bulk rename review: allow or deny each row, with warnings for extension changes and rename cycles.',
              'The operation log records who started an operation: you, an AI client, or the agent.',
            ],
          },
          {
            title: 'Fixed',
            entriesHtml: ['Copying to a disconnected network share now explains itself instead of stalling.'],
          },
        ],
      },
    ],
  },
  // An update that fixed two security issues: the notice above the releases, one link per
  // advisory, most severe first. The ids and links are placeholders that pass the allowlist.
  'security-notice': {
    open: true,
    allowEmpty: false,
    advisories: [
      {
        id: 'GHSA-xxxx-xxxx-xxxx',
        fixedIn: '0.31.0',
        severity: 'high',
        url: 'https://github.com/vdavid/cmdr/security/advisories/GHSA-xxxx-xxxx-xxxx',
      },
      { id: '2026-07-10-example', fixedIn: '0.30.0', severity: 'low', url: 'https://getcmdr.com/changelog' },
    ],
    releases: [
      {
        version: '0.31.0',
        date: '2026-07-20',
        leadHtml:
          '<p><strong>Ask Cmdr can rename in bulk now.</strong> Describe the naming you want and review every row before anything touches disk.</p>\n',
        sections: [{ title: 'Security', entriesHtml: ['Fixed a path check that a crafted archive could slip past.'] }],
      },
    ],
  },
  // Changelogs turned off: the notice is the whole dialog.
  'security-notice-only': {
    open: true,
    allowEmpty: false,
    releases: [],
    advisories: [
      {
        id: 'GHSA-xxxx-xxxx-xxxx',
        fixedIn: '0.31.0',
        severity: 'critical',
        url: 'https://github.com/vdavid/cmdr/security/advisories/GHSA-xxxx-xxxx-xxxx',
      },
    ],
  },
  // The realistic post-update case: several releases, long entries, and a lead with
  // two paragraphs and a numbered list nesting a bulleted one. The HTML is exactly what
  // the backend's CommonMark renderer (`src-tauri/src/whats_new/`) emits for that shape.
  'several-releases': {
    open: true,
    allowEmpty: false,
    releases: [
      {
        version: '0.31.0',
        date: '2026-07-20',
        // The link is here so the gallery shows a clickable one: Tauri blocks raw <a>
        // navigation, so the dialog routes it through `handleMarkdownLinkClick`. It sits in
        // the lead because the backend flattens links in entries to their text.
        leadHtml:
          '<p><strong>Two big ones this time.</strong> Thanks for all the feedback, and the rest is in the <a href="https://getcmdr.com/changelog/">changelog</a>.</p>\n<p>Some highlights:</p>\n<ol>\n<li>Ask Cmdr can rename in bulk, with a review step.</li>\n<li>The file viewer opens 4 GB logs without breaking a sweat, and <code>⌘F</code> searches inside them.</li>\n<li>Context menu updates:\n<ul>\n<li>On files: <em>Share</em> and <em>Services</em> menus.</li>\n<li>On Cmdr in your Dock: your tabs, bookmarks, and servers.</li>\n</ul>\n</li>\n</ol>\n',
        sections: [
          {
            title: 'Added',
            entriesHtml: [
              'Bulk rename review: allow or deny each row, with warnings for extension changes and rename cycles.',
              'The file viewer streams big files instead of loading them, so a 4 GB log opens as fast as a 4 KB one.',
              'Volume tints: give each drive a colour so you always know which pane you’re in.',
            ],
          },
          {
            title: 'Changed',
            entriesHtml: [
              'The transfer dialog shows real throughput and an honest ETA, and both keep updating while the scan is still running.',
            ],
          },
          {
            title: 'Fixed',
            // Inline markup on purpose. Roughly one changelog entry in twelve carries a
            // `code` span, and an entry's markup has to stay in the text flow instead of
            // wrapping into the bullet column; `whats-new-markup.spec.ts` measures these.
            entriesHtml: [
              'Copying to a disconnected network share explains itself instead of stalling.',
              'Shares named <code>café</code> or <code>公開</code> mount again, and a <code>diskutil</code> message stops reaching a toast in English.',
              'The crash dialog stops claiming Cmdr quit <strong>unexpectedly</strong> when it didn’t, and stops saying it twice.',
              'Mounting a share over a flaky link retries instead of giving up.',
            ],
          },
        ],
      },
      {
        version: '0.30.2',
        date: '2026-07-11',
        leadHtml: null,
        sections: [
          {
            title: 'Fixed',
            entriesHtml: [
              'MTP devices reconnect after sleep instead of showing an empty pane.',
              'The search index no longer re-scans an external drive that never went stale.',
            ],
          },
          {
            title: 'Security',
            entriesHtml: ['Archive extraction rejects entries that would escape the destination folder.'],
          },
        ],
      },
      {
        version: '0.30.1',
        date: '2026-07-03',
        leadHtml: '<p>A quiet one: mostly indexing throughput.</p>\n',
        sections: [
          {
            title: 'Changed',
            entriesHtml: ['Indexing a big drive uses about a third of the memory it used to.'],
          },
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
    inverseOpId: null,
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
    case 'suggested-ops':
      // A builder per state: the reviews it seeds are live maps the dialog mutates.
      return seedFrom(
        suggestedOpsState,
        stateId in suggestedOpsFixtures ? suggestedOpsFixtures[stateId]() : undefined,
        () => suggestedOpsState.open,
      )
    case 'whats-new':
      return seedFrom(whatsNewState, whatsNewFixtures[stateId], () => whatsNewState.open)
  }
}
