/**
 * Fixtures for `sources-gone` (`#lib/file-operations/SourcesGoneDialog.svelte`).
 *
 * A queued operation's question when some of its sources left while it waited.
 * The answers live in the `onContinue` / `onCancel` props, which the gallery
 * leaves to closing the preview, so the question itself is the whole state: the
 * operation kind picks the sentence and the button verb, and the count picks
 * whether the name list ends in "and N more".
 */

import type { OperationSnapshot, WriteSourcesGoneEvent } from '#lib/tauri-commands/index.js'

export interface SourcesGoneFixture {
  event: WriteSourcesGoneEvent
  snapshot: OperationSnapshot
}

function fixture(
  operationType: WriteSourcesGoneEvent['operationType'],
  goneSample: string[],
  goneCount: number,
  totalCount: number,
  destination: string | null,
): SourcesGoneFixture {
  return {
    event: { operationId: 'gallery-op', operationType, totalCount, goneCount, goneSample, next: 'awaiting_answer' },
    snapshot: {
      operationId: 'gallery-op',
      operationType,
      status: 'running',
      source: 'CD1',
      destination,
      // What the backend would say for each fixture: the moves go to a NAS.
      rollbackSupport:
        operationType === 'copy' ? 'supported' : operationType === 'move' ? 'crossVolumeMove' : 'nothingToPutBack',
      reverses: null,
      error: null,
      canRetry: false,
      conflictPolicy: null,
      startedAt: null,
      items: null,
    },
  }
}

const tracks = (count: number) =>
  Array.from({ length: count }, (_, i) => `/Users/me/Music/Album/CD1/${String(i + 1).padStart(2, '0')} - Track.mp3`)

export const sourcesGoneFixtures: Record<string, SourcesGoneFixture | undefined> = {
  'move-some': fixture('move', tracks(4), 4, 22, 'Naspolya Music'),
  'move-many': fixture('move', tracks(5), 1_234, 5_678, 'Naspolya Music'),
  'copy-one': fixture('copy', ['/Users/me/Documents/Invoices 2026'], 1, 3, 'Backup'),
  trash: fixture('trash', tracks(2), 2, 9, null),
}
