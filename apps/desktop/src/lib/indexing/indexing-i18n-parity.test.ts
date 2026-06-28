/**
 * Base-locale (en) parity net for the indexing i18n migration.
 *
 * The drive-indexing status labels, ETA phrases, and rescan-reason toasts moved
 * from hardcoded English into the `indexing.*` catalog. This is a
 * behavior-preserving MOVE: every rendered en string must be byte-identical to
 * the pre-migration copy. These goldens are the literals rendered by the
 * indexing indicator (`IndexingStatusIndicator.svelte` / `IndexingDriveRow.svelte`),
 * `eta.ts`, and `index-state.svelte.ts`; if a future copy edit is intended, it
 * lands in the catalog AND here together, never silently.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { tString } from '$lib/intl/messages.svelte'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('indexing catalog parity (en)', () => {
  it('resolves the scan labels and counters', () => {
    expect(tString('indexing.status.ariaLabel')).toBe('Drive indexing status')
    expect(tString('indexing.scan.label')).toBe('Scanning your drive...')
    expect(tString('indexing.scan.counters', { entriesText: '12,345', dirsText: '678' })).toBe(
      '12,345 entries, 678 dirs',
    )
    expect(tString('indexing.scan.etaRough', { eta: '2m left' })).toBe('roughly 2m left')
    expect(tString('indexing.drive.heading', { name: 'Macintosh HD' })).toBe('Macintosh HD')
  })

  it('resolves the checklist step labels', () => {
    expect(tString('indexing.step.findFiles')).toBe('Find files')
    expect(tString('indexing.step.saveFileList')).toBe('Save the file list')
    expect(tString('indexing.step.computeFolderSizes')).toBe('Compute folder sizes')
    expect(tString('indexing.step.catchUp')).toBe('Catch up on recent changes')
    expect(tString('indexing.step.updateIndex')).toBe('Update index')
    expect(tString('indexing.step.findFilesFirstScan')).toBe('First scan, so this can take a while')
    expect(tString('indexing.step.statusDone')).toBe('Done')
    expect(tString('indexing.step.statusActive')).toBe('In progress')
    expect(tString('indexing.step.statusPending')).toBe('Not started')
    expect(tString('indexing.summary.found', { countText: '171,607' })).toBe('171,607 found')
  })

  it('resolves the compute-step sub-phase labels (folder-worded)', () => {
    expect(tString('indexing.aggregation.loading')).toBe('Loading folders...')
    expect(tString('indexing.aggregation.sorting')).toBe('Sorting folders...')
    expect(tString('indexing.aggregation.computing')).toBe('Computing folder sizes...')
    expect(tString('indexing.aggregation.writing')).toBe('Saving folder sizes...')
  })

  it('resolves the replay detail', () => {
    expect(tString('indexing.replay.detail', { eventsText: '1,234' })).toBe('1,234 events processed')
  })

  it('resolves the ETA phrases (preserving the s/m abbreviations)', () => {
    expect(tString('indexing.eta.almostDone')).toBe('Almost done')
    expect(tString('indexing.eta.secondsLeft', { secondsText: '45' })).toBe('45s left')
    expect(tString('indexing.eta.minutesLeft', { minutesText: '3' })).toBe('3m left')
  })

  it('resolves every rescan-reason message with apostrophes intact', () => {
    expect(tString('indexing.rescan.staleIndex')).toBe(
      "Your drive index is outdated. It looks like the app hasn't run for a while. Running a fresh scan to catch up.",
    )
    expect(tString('indexing.rescan.journalGap')).toBe(
      "The system's file change log doesn't go back far enough. Running a fresh scan to rebuild the index.",
    )
    expect(tString('indexing.rescan.replayOverflow')).toBe(
      'A lot of file changes happened since last run. Running a fresh scan instead of replaying them one by one.',
    )
    expect(tString('indexing.rescan.tooManySubdirRescans')).toBe(
      'Many directories changed significantly since last run. Running a fresh scan to get everything up to date.',
    )
    expect(tString('indexing.rescan.watcherStartFailed')).toBe(
      "Couldn't start the file change watcher. Running a fresh scan to get the index up to date.",
    )
    expect(tString('indexing.rescan.reconcilerBufferOverflow')).toBe(
      'Heavy filesystem activity overwhelmed the event buffer. Running a fresh scan to stay accurate.',
    )
    expect(tString('indexing.rescan.incompletePreviousScan')).toBe(
      "The previous scan didn't finish (the app may have been closed). Restarting the scan from scratch.",
    )
    expect(tString('indexing.rescan.watcherChannelOverflow')).toBe(
      'A burst of filesystem activity overflowed the watcher channel. Running a fresh scan to stay accurate.',
    )
    expect(tString('indexing.rescan.fallback')).toBe('Running a fresh drive scan to keep the index accurate.')
  })
})
