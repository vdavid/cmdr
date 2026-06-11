import { describe, it, expect, vi, afterEach } from 'vitest'
import type { SourceResult } from './types.js'
import { withTimeout } from './fetch-all.js'

describe('withTimeout', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  it('passes through a result that settles in time', async () => {
    const result = await withTimeout('Test', Promise.resolve({ ok: true, data: 42 } as SourceResult<number>))
    expect(result).toEqual({ ok: true, data: 42 })
  })

  it('passes through an error result that settles in time', async () => {
    const result = await withTimeout(
      'Test',
      Promise.resolve({ ok: false, error: 'Test: boom' } as SourceResult<number>),
    )
    expect(result).toEqual({ ok: false, error: 'Test: boom' })
  })

  it('resolves to a timeout error when the source hangs', async () => {
    vi.useFakeTimers()
    const hung = new Promise<SourceResult<number>>(() => {
      // Never settles, like a fetch to a hung upstream (Workers fetch has no built-in timeout)
    })
    const resultPromise = withTimeout('Umami', hung)
    await vi.advanceTimersByTimeAsync(20_000)
    expect(await resultPromise).toEqual({ ok: false, error: 'Umami: timed out after 20s' })
  })

  it('does not time out a source that settles just before the deadline', async () => {
    vi.useFakeTimers()
    let settle: (value: SourceResult<string>) => void = () => {}
    const slow = new Promise<SourceResult<string>>((resolve) => {
      settle = resolve
    })
    const resultPromise = withTimeout('Paddle', slow)
    await vi.advanceTimersByTimeAsync(19_999)
    settle({ ok: true, data: 'made it' })
    expect(await resultPromise).toEqual({ ok: true, data: 'made it' })
  })
})
