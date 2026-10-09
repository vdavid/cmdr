import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { OperationDetails } from '$lib/ipc/bindings'
import { getOperationDetails } from '$lib/tauri-commands'
import { createOperationDetailsLoader } from './operation-details.svelte'

vi.mock('$lib/tauri-commands', () => ({
  getOperationDetails: vi.fn(),
}))

const logWarn = vi.hoisted(() => vi.fn())
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ error: vi.fn(), debug: vi.fn(), warn: logWarn, info: vi.fn() }),
}))

const fetchDetails = vi.mocked(getOperationDetails)

function detailsFor(operationId: string, sourcePaths: string[] = ['/a/one.txt']): OperationDetails {
  return {
    operationId,
    sourcePaths,
    sourceCount: sourcePaths.length,
    destinationPath: '/b',
    sourceVolumeName: null,
    destinationVolumeName: null,
    queuedAt: 1_700_000_000,
    startedAt: 1_700_000_005,
  }
}

/** A promise the test resolves by hand, to land answers out of order. */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void; reject: (error: unknown) => void } {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

const tick = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0))

describe('createOperationDetailsLoader', () => {
  beforeEach(() => {
    fetchDetails.mockReset()
    logWarn.mockReset()
  })

  it('holds the answer for the operation it asked about', async () => {
    fetchDetails.mockResolvedValueOnce(detailsFor('op-1'))
    const loader = createOperationDetailsLoader()

    loader.load('op-1')
    await tick()

    expect(fetchDetails).toHaveBeenCalledWith('op-1')
    expect(loader.details?.sourcePaths).toEqual(['/a/one.txt'])
    expect(loader.unavailable).toBe(false)
  })

  it('drops an answer that a newer request has overtaken', async () => {
    const first = deferred<OperationDetails | null>()
    const second = deferred<OperationDetails | null>()
    fetchDetails.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    const loader = createOperationDetailsLoader()

    loader.load('op-1')
    loader.load('op-1')
    second.resolve(detailsFor('op-1', ['/newer']))
    await tick()
    first.resolve(detailsFor('op-1', ['/older']))
    await tick()

    expect(loader.details?.sourcePaths).toEqual(['/newer'])
  })

  it('never shows one operation the answer about another', async () => {
    fetchDetails.mockResolvedValueOnce(detailsFor('op-other'))
    const loader = createOperationDetailsLoader()

    loader.load('op-1')
    await tick()

    expect(loader.details).toBeNull()
  })

  it('treats a gone operation as nothing to show, not a fault', async () => {
    fetchDetails.mockResolvedValueOnce(null)
    const loader = createOperationDetailsLoader()

    loader.load('op-1')
    await tick()

    expect(loader.details).toBeNull()
    expect(loader.unavailable).toBe(false)
    expect(logWarn).not.toHaveBeenCalled()
  })

  it('logs a broken bridge and says the details are unavailable', async () => {
    fetchDetails.mockRejectedValueOnce(new Error('not allowed'))
    const loader = createOperationDetailsLoader()

    loader.load('op-1')
    await tick()

    expect(loader.unavailable).toBe(true)
    expect(logWarn).toHaveBeenCalledTimes(1)
  })

  it('keeps the last good answer while a refresh is in flight', async () => {
    const refresh = deferred<OperationDetails | null>()
    fetchDetails.mockResolvedValueOnce(detailsFor('op-1')).mockReturnValueOnce(refresh.promise)
    const loader = createOperationDetailsLoader()

    loader.load('op-1')
    await tick()
    loader.load('op-1')

    expect(loader.details?.operationId).toBe('op-1')
    refresh.resolve(detailsFor('op-1', ['/refreshed']))
    await tick()
    expect(loader.details?.sourcePaths).toEqual(['/refreshed'])
  })

  it('ignores an answer that lands after the row went away', async () => {
    const late = deferred<OperationDetails | null>()
    fetchDetails.mockReturnValueOnce(late.promise)
    const loader = createOperationDetailsLoader()

    loader.load('op-1')
    loader.dispose()
    late.resolve(detailsFor('op-1'))
    await tick()

    expect(loader.details).toBeNull()
  })
})
