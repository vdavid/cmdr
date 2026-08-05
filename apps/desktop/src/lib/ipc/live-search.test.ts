/**
 * IPC contract tests for the live-search surface: `search_files_streaming`,
 * `cancel_search`, and the four events they report through.
 *
 * A live search is the one search that answers over TIME, and every part of that
 * contract is a place drift is silent. The run id ties events to the query that
 * asked for them (a refined query supersedes the previous run rather than
 * cancelling its walk), so a renamed field or a mistyped event name would leave
 * results arriving against a run the frontend has dropped: no error, no results,
 * nothing in a log. The backend has its own tests for what the run DOES
 * (`src-tauri/src/search/live/tests.rs`); these pin the wire between them.
 */

import { afterEach, describe, expect, it } from 'vitest'

import { commands, events } from '$lib/ipc/bindings'
import type {
  LiveSearchStart,
  SearchCancelledEvent,
  SearchCompleteEvent,
  SearchErrorEvent,
  SearchProgressEvent,
  SearchQuery,
} from '$lib/ipc/bindings'
import { clearIpcMocks, installIpcMock } from '$lib/ipc/test-helpers'

afterEach(() => {
  clearIpcMocks()
})

const query: SearchQuery = {
  namePattern: 'report',
  patternType: 'glob',
  minSize: null,
  maxSize: null,
  modifiedAfter: null,
  modifiedBefore: null,
  isDirectory: null,
  includePaths: ['/Users/me/Documents'],
  excludeDirNames: null,
  limit: 30,
  caseSensitive: null,
  excludeSystemDirs: null,
  countOnly: false,
}

const started: LiveSearchStart = {
  runId: 'run-1',
  targetVolumeId: 'root',
}

describe('commands.searchFilesStreaming', () => {
  it('sends the query and the CALLER-supplied run id, and names the volume it routed to', async () => {
    // The caller supplies the run id (as it does a listing id) so no event can
    // arrive against an id the frontend hasn't seen yet.
    const ipc = installIpcMock()
    ipc.mock('search_files_streaming', () => started)

    const result = await commands.searchFilesStreaming(query, 'run-1')

    expect(result).toEqual({ status: 'ok', data: started })
    expect(ipc.lastCall('search_files_streaming')?.payload).toEqual({ query, runId: 'run-1' })
  })

  it('surfaces a refused scope as an error rather than an empty answer', async () => {
    // One volume is the ceiling, enforced at the API: a scope spanning two of them
    // is refused before any run starts, so it comes back on the error branch of
    // the start call and never as a silent zero-result run.
    const ipc = installIpcMock()
    ipc.mock('search_files_streaming', () => {
      throw 'A search covers one volume at a time.'
    })

    const result = await commands.searchFilesStreaming(query, 'run-2')

    expect(result.status).toBe('error')
  })
})

describe('commands.cancelSearch', () => {
  it('sends the run id and reports whether there was a run to stop', async () => {
    const ipc = installIpcMock()
    ipc.mock('cancel_search', () => true)

    const result = await commands.cancelSearch('run-1')

    expect(result).toEqual({ status: 'ok', data: true })
    expect(ipc.lastCall('cancel_search')?.payload).toEqual({ runId: 'run-1' })
  })
})

describe('the live-search event family', () => {
  it('is bound in the generated surface, all four of it', () => {
    // The wire NAMES are pinned Rust-side (`live::tests::the_event_family_keeps_
    // its_wire_names`), where the emitter is; what belongs here is that the
    // generated surface actually carries all four, so a family that lost one in a
    // regen fails a test rather than a user's search.
    for (const event of [events.searchProgress, events.searchComplete, events.searchCancelled, events.searchError]) {
      expect(typeof event.listen).toBe('function')
    }
  })

  it('carries a run id on every event, terminal ones included', () => {
    // Typed rather than asserted at runtime: the compiler is what enforces it, and
    // this is the case that fails silently — a batch landing against a superseded
    // run's id has to be droppable without inspecting anything else.
    const progress: SearchProgressEvent = {
      runId: 'run-1',
      phase: 'walking',
      entries: [],
      matchCount: 7,
      dirsFound: 42,
      currentPath: '/Users/me/Documents/2019',
      capped: false,
    }
    const complete: SearchCompleteEvent = {
      runId: 'run-1',
      matchCount: 7,
      coverage: {
        walk: 'completed',
        unreadable: [],
        stillCovering: [],
        unresolvedScopes: [],
        capped: false,
        targetVolumeId: 'root',
      },
    }
    const cancelled: SearchCancelledEvent = { ...complete, coverage: { ...complete.coverage, walk: 'cancelled' } }
    const failed: SearchErrorEvent = {
      runId: 'run-1',
      error: 'query',
      message: 'Query too broad. Add a filename pattern, size, date, or type filter to narrow results.',
    }

    for (const event of [progress, complete, cancelled, failed]) {
      expect(event.runId).toBe('run-1')
    }
  })

  it('types the four ways a walk can end, so incomplete never reads as exhaustive', () => {
    // Three of the four leave the list a lower bound, and the copy differs per
    // reason: stopped by the user, stopped by a drive going away, or nothing to
    // walk in the first place.
    const endings: SearchCompleteEvent['coverage']['walk'][] = [
      'nothingToWalk',
      'completed',
      'interrupted',
      'cancelled',
    ]
    expect(new Set(endings).size).toBe(4)
  })
})
