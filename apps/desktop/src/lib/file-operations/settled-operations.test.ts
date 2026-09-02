/**
 * Two umbrella-level modules that live off the same `$lib/tauri-commands` event
 * stream, so they share this file's one mock of it: `settled-operations` (below)
 * and `NewEntryNameCheck` (at the bottom), the name validation behind the New
 * folder and New file dialogs.
 *
 * Waiting for an operation to settle, and the one property the whole thing turns
 * on: **a settle that already happened still answers.**
 *
 * `write-settled` follows its terminal event by microseconds, while the frontend
 * holds its own completion handling for up to `MIN_DISPLAY_MS`. So by the time
 * anything asks, the event is almost always in the past. A wait with no memory
 * of it would time out every time and the follow-up would silently never run,
 * which is exactly the failure this module was written to end.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { WriteSettledEvent } from '$lib/tauri-commands'

const { onWriteSettled, findFileIndex, getFileAt, onDirectoryDiff } = vi.hoisted(() => ({
  onWriteSettled: vi.fn(),
  findFileIndex: vi.fn(),
  getFileAt: vi.fn(),
  onDirectoryDiff: vi.fn(),
}))
vi.mock('$lib/tauri-commands', () => ({ onWriteSettled, findFileIndex, getFileAt, onDirectoryDiff }))

import { initSettledOperationsWatch, destroySettledOperationsWatch, whenOperationSettled } from './settled-operations'
import { NewEntryNameCheck, type NewEntryNameCheckOptions } from './new-entry-name-check.svelte'
import type { DirectoryDiff } from '$lib/file-explorer/types'

/** Feeds the module a settle event, the way the backend stream would. */
let emitSettled: (event: WriteSettledEvent) => void = () => {}
const unlisten = vi.fn()

function settledEvent(operationId: string): WriteSettledEvent {
  return { operationId, operationType: 'copy' }
}

beforeEach(async () => {
  vi.clearAllMocks()
  vi.useFakeTimers()
  onWriteSettled.mockImplementation((callback: (event: WriteSettledEvent) => void) => {
    emitSettled = callback
    return Promise.resolve(unlisten)
  })
  await initSettledOperationsWatch()
})

afterEach(() => {
  destroySettledOperationsWatch()
  vi.useRealTimers()
})

describe('whenOperationSettled', () => {
  it('answers immediately for an operation that settled BEFORE anyone asked', async () => {
    emitSettled(settledEvent('op-1'))

    await expect(whenOperationSettled('op-1')).resolves.toBe(true)
  })

  it('answers when the settle lands afterwards', async () => {
    const waiting = whenOperationSettled('op-2')
    emitSettled(settledEvent('op-2'))

    await expect(waiting).resolves.toBe(true)
  })

  it('gives up after the timeout when the settle never comes', async () => {
    const waiting = whenOperationSettled('op-never')
    await vi.advanceTimersByTimeAsync(5000)

    await expect(waiting).resolves.toBe(false)
  })

  it('releases every waiter on one id, and leaves other ids waiting', async () => {
    const first = whenOperationSettled('op-3')
    const second = whenOperationSettled('op-3')
    const other = whenOperationSettled('op-4')
    let otherAnswered = false
    void other.then(() => (otherAnswered = true))

    emitSettled(settledEvent('op-3'))

    await expect(first).resolves.toBe(true)
    await expect(second).resolves.toBe(true)
    expect(otherAnswered).toBe(false)
    await vi.advanceTimersByTimeAsync(5000)
    await expect(other).resolves.toBe(false)
  })

  it('forgets an id once 64 later operations have settled, so the memory stays bounded', async () => {
    emitSettled(settledEvent('op-old'))
    for (let i = 0; i < 64; i++) emitSettled(settledEvent(`op-${String(i)}`))

    const stale = whenOperationSettled('op-old')
    await vi.advanceTimersByTimeAsync(5000)
    await expect(stale).resolves.toBe(false)
    // The newest ones are still remembered.
    await expect(whenOperationSettled('op-63')).resolves.toBe(true)
  })

  it('is idempotent: a second init does not add a second listener', async () => {
    await initSettledOperationsWatch()

    expect(onWriteSettled).toHaveBeenCalledTimes(1)
  })

  it('answers waiters with false when the window tears the watch down', async () => {
    const waiting = whenOperationSettled('op-5')

    destroySettledOperationsWatch()

    await expect(waiting).resolves.toBe(false)
    expect(unlisten).toHaveBeenCalled()
  })
})

/**
 * The name typed into New folder / New file: sync rules first, then one clash
 * lookup against the live listing.
 *
 * The order is the point. A name the file system would refuse must be named for
 * what it is before anything asks the listing about it, and a lookup that can't
 * answer (the listing went away underneath the dialog) must leave the field clear
 * so the backend gets the final word instead of the dialog inventing one.
 */
describe('NewEntryNameCheck', () => {
  const LISTING = 'listing-1'
  const CURRENT_PATH = '/Users/me/Documents'

  /** What the field holds, as `getName` reads it when the debounce fires. */
  let typed = ''
  let unlistenDiff = vi.fn()
  let emitDiff: (payload: DirectoryDiff) => void = () => {}

  function makeCheck(overrides: Partial<NewEntryNameCheckOptions> = {}): NewEntryNameCheck {
    return new NewEntryNameCheck({
      currentPath: CURRENT_PATH,
      listingId: LISTING,
      showHiddenFiles: false,
      getName: () => typed,
      ...overrides,
    })
  }

  function diff(listingId: string): DirectoryDiff {
    return { listingId, sequence: 1, changes: [] }
  }

  beforeEach(() => {
    typed = ''
    unlistenDiff = vi.fn()
    emitDiff = () => {}
    findFileIndex.mockResolvedValue(null)
    getFileAt.mockResolvedValue(null)
    onDirectoryDiff.mockImplementation((handler: (payload: DirectoryDiff) => void) => {
      emitDiff = handler
      return Promise.resolve(unlistenDiff)
    })
  })

  it('leaves an empty name alone: nothing to say yet, and nothing to look up', async () => {
    const check = makeCheck()
    check.errorMessage = 'left over from an earlier keystroke'

    await check.validate('   ')

    expect(check.errorMessage).toBe('')
    expect(findFileIndex).not.toHaveBeenCalled()
  })

  it('names a disallowed character and stops before the lookup', async () => {
    const check = makeCheck()

    await check.validate('holiday/photos')

    expect(check.errorMessage).toMatch(/null characters/)
    expect(findFileIndex).not.toHaveBeenCalled()
  })

  it('names a too-long name and stops before the lookup', async () => {
    const check = makeCheck()

    await check.validate('a'.repeat(300))

    expect(check.errorMessage).toMatch(/too long/)
    expect(findFileIndex).not.toHaveBeenCalled()
  })

  it('names a too-long full path even when the name itself fits', async () => {
    const check = makeCheck({ currentPath: '/' + 'deep/'.repeat(220) })

    await check.validate('notes.txt')

    expect(check.errorMessage).toMatch(/Full path is too long/)
    expect(findFileIndex).not.toHaveBeenCalled()
  })

  it('calls the clash out as a folder when the listing holds a folder by that name', async () => {
    findFileIndex.mockResolvedValue(7)
    getFileAt.mockResolvedValue({ isDirectory: true })
    const check = makeCheck()

    await check.validate('Photos')

    expect(findFileIndex).toHaveBeenCalledWith(LISTING, 'Photos', false)
    expect(getFileAt).toHaveBeenCalledWith(LISTING, 7, false)
    expect(check.errorMessage).toBe('There is already a folder by this name in this folder.')
    expect(check.isChecking).toBe(false)
  })

  it('calls the clash out as a file otherwise, the entry itself gone included', async () => {
    findFileIndex.mockResolvedValue(2)
    getFileAt.mockResolvedValue({ isDirectory: false })
    const check = makeCheck()

    await check.validate('notes.txt')
    expect(check.errorMessage).toBe('There is already a file by this name in this folder.')

    // The index answered but the entry didn't: still a clash, worded as a file.
    getFileAt.mockResolvedValue(null)
    await check.validate('notes.txt')
    expect(check.errorMessage).toBe('There is already a file by this name in this folder.')
  })

  it('clears a stale message once the name is free', async () => {
    const check = makeCheck()
    check.errorMessage = 'There is already a file by this name in this folder.'

    await check.validate('Brand new folder')

    expect(check.errorMessage).toBe('')
    expect(getFileAt).not.toHaveBeenCalled()
  })

  it('leaves the field clear when the lookup itself cannot answer, so the backend decides', async () => {
    findFileIndex.mockRejectedValue(new Error('listing gone'))
    const check = makeCheck()
    check.errorMessage = 'There is already a file by this name in this folder.'

    await check.validate('Photos')

    expect(check.errorMessage).toBe('')
    expect(check.isChecking).toBe(false)
  })

  it('holds `isChecking` up for as long as the lookup is in flight, so OK stays held', async () => {
    let answer: (index: number | null) => void = () => {}
    findFileIndex.mockReturnValue(
      new Promise<number | null>((resolve) => {
        answer = resolve
      }),
    )
    const check = makeCheck()

    const pending = check.validate('Photos')
    expect(check.isChecking).toBe(true)

    answer(null)
    await pending
    expect(check.isChecking).toBe(false)
  })

  it('coalesces keystrokes and validates the LATEST name, once', async () => {
    const check = makeCheck({ showHiddenFiles: true })

    typed = 'Pho'
    check.schedule()
    typed = 'Photos'
    check.schedule()
    await vi.advanceTimersByTimeAsync(100)

    expect(findFileIndex).toHaveBeenCalledTimes(1)
    expect(findFileIndex).toHaveBeenCalledWith(LISTING, 'Photos', true)
  })

  it('re-checks when a diff lands for this listing, and ignores every other listing', async () => {
    const check = makeCheck()
    await check.listen()
    typed = 'Photos'

    emitDiff(diff(LISTING))
    await vi.advanceTimersByTimeAsync(100)
    expect(findFileIndex).toHaveBeenCalledTimes(1)

    findFileIndex.mockClear()
    emitDiff(diff('some-other-listing'))
    await vi.advanceTimersByTimeAsync(100)
    expect(findFileIndex).not.toHaveBeenCalled()
  })

  it('drops the pending re-check and the diff subscription when the dialog goes away', async () => {
    const check = makeCheck()
    await check.listen()
    typed = 'Photos'
    check.schedule()

    check.dispose()
    await vi.advanceTimersByTimeAsync(100)

    expect(findFileIndex).not.toHaveBeenCalled()
    expect(unlistenDiff).toHaveBeenCalledTimes(1)
  })

  it('disposes cleanly when it never scheduled or listened', () => {
    expect(() => {
      makeCheck().dispose()
    }).not.toThrow()
    expect(unlistenDiff).not.toHaveBeenCalled()
  })
})
