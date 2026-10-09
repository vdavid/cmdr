import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import QueueRowDetails from './QueueRowDetails.svelte'
import type { OperationDetails, OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'
import { getOperationDetails } from '$lib/tauri-commands'

vi.mock('$lib/tauri-commands', () => ({
  getOperationDetails: vi.fn(),
}))

// Dates go through the user's date-format setting; a stub keeps the assertion
// about WHICH timestamp lands where, not about how it's spelled.
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatDateTime: (unixSeconds: number) => `at-${String(unixSeconds)}`,
  getFileSizeFormat: () => 'decimal',
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ error: vi.fn(), debug: vi.fn(), warn: vi.fn(), info: vi.fn() }),
}))

const fetchDetails = vi.mocked(getOperationDetails)

const STARTED = 1_700_000_100

function snapshotWith(status: OperationSnapshot['status']): OperationSnapshot {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    status,
    source: 'a.raw (5 items)',
    destination: 'backup',
    supportsRollback: true,
    reverses: null,
    error: null,
  }
}

function detailsWith(overrides: Partial<OperationDetails> = {}): OperationDetails {
  return {
    operationId: 'op-1',
    sourcePaths: ['/Users/me/photos/a.raw', '/Users/me/photos/b.raw'],
    sourceCount: 5,
    destinationPath: '/Volumes/Naspolya/backup',
    queuedAt: 1_700_000_000,
    startedAt: STARTED,
    ...overrides,
  }
}

const copying: WriteProgressEvent = {
  operationId: 'op-1',
  operationType: 'copy',
  phase: 'copying',
  currentFile: 'b.raw',
  filesDone: 1,
  filesTotal: 5,
  bytesDone: 10,
  bytesTotal: 50,
}

const settle = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0))

let target: HTMLElement
let instance: ReturnType<typeof mount> | undefined
const props = $state<{ snapshot: OperationSnapshot; progress: WriteProgressEvent | null; id: string }>({
  snapshot: snapshotWith('running'),
  progress: copying,
  id: 'details-op-1',
})

async function render(snapshot: OperationSnapshot, progress: WriteProgressEvent | null): Promise<void> {
  props.snapshot = snapshot
  props.progress = progress
  target = document.createElement('div')
  document.body.appendChild(target)
  instance = mount(QueueRowDetails, { target, props })
  flushSync()
  await settle()
  flushSync()
}

/** The panel's facts as `label → value` text, in order. */
function facts(): Record<string, string> {
  const result: Record<string, string> = {}
  for (const term of target.querySelectorAll('dt')) {
    const value = term.nextElementSibling
    result[term.textContent.trim()] = value?.textContent.replace(/\s+/g, ' ').trim() ?? ''
  }
  return result
}

beforeEach(() => {
  document.body.innerHTML = ''
  fetchDetails.mockReset()
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(new Date((STARTED + 150) * 1000))
})

afterEach(() => {
  if (instance) void unmount(instance)
  instance = undefined
  vi.useRealTimers()
})

describe('QueueRowDetails', () => {
  it('lays out every source in full, the destination, the file in flight, and the timing', async () => {
    fetchDetails.mockResolvedValue(detailsWith())
    await render(snapshotWith('running'), copying)

    expect(fetchDetails).toHaveBeenCalledWith('op-1')
    const sources = [...target.querySelectorAll('.sources li')].map((li) => li.textContent.trim())
    expect(sources).toEqual(['/Users/me/photos/a.raw', '/Users/me/photos/b.raw', 'and 3 more'])
    expect(facts()).toMatchObject({
      To: '/Volumes/Naspolya/backup',
      'Current file': 'b.raw',
      Started: `at-${String(STARTED)}`,
      Elapsed: '2m 30s',
    })
    expect(target.firstElementChild?.id).toBe('details-op-1')
  })

  it('says when a waiting operation joined the queue, and nothing about a start', async () => {
    fetchDetails.mockResolvedValue(detailsWith({ startedAt: null }))
    await render(snapshotWith('queued'), null)

    const shown = facts()
    expect(shown['Waiting since']).toBe('at-1700000000')
    expect(shown).not.toHaveProperty('Started')
    expect(shown).not.toHaveProperty('Elapsed')
    expect(shown).not.toHaveProperty('Current file')
    expect(shown.To).toBe('/Volumes/Naspolya/backup')
  })

  it('keeps a failed row to where it was going and when it started', async () => {
    fetchDetails.mockResolvedValue(detailsWith())
    await render(snapshotWith('failed'), copying)

    const shown = facts()
    expect(shown.Started).toBe(`at-${String(STARTED)}`)
    expect(shown).not.toHaveProperty('Elapsed')
    expect(shown).not.toHaveProperty('Current file')
  })

  it('asks again when the status changes, and not when the snapshot is merely rebuilt', async () => {
    fetchDetails.mockResolvedValue(detailsWith({ startedAt: null }))
    await render(snapshotWith('queued'), null)
    expect(fetchDetails).toHaveBeenCalledTimes(1)

    props.snapshot = snapshotWith('queued')
    flushSync()
    expect(fetchDetails, 'a rebuild with nothing new about this row').toHaveBeenCalledTimes(1)

    fetchDetails.mockResolvedValue(detailsWith())
    props.snapshot = snapshotWith('running')
    flushSync()
    await settle()
    flushSync()
    expect(fetchDetails).toHaveBeenCalledTimes(2)
    expect(facts().Started).toBe(`at-${String(STARTED)}`)
  })

  it('says the details are unavailable when the lookup itself breaks', async () => {
    fetchDetails.mockRejectedValue(new Error('not allowed'))
    await render(snapshotWith('running'), copying)

    expect(target.textContent).toContain('Details aren’t available right now.')
  })

  it('shows nothing it would have to guess when the operation is already gone', async () => {
    fetchDetails.mockResolvedValue(null)
    await render(snapshotWith('running'), copying)

    expect(target.querySelector('.sources')).toBeNull()
    expect(target.textContent).not.toContain('Details aren’t available')
    expect(facts()['Current file'], 'the live tick still names the file in flight').toBe('b.raw')
  })
})
