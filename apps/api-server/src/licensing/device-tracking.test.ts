import { describe, expect, it, vi } from 'vitest'
import { pruneStaleDevices, shouldAlert } from './device-tracking'

describe('pruneStaleDevices', () => {
  it('removes entries older than maxAgeDays', () => {
    const now = Date.now()
    const devices: Record<string, string> = {
      'device-old': new Date(now - 91 * 24 * 60 * 60 * 1000).toISOString(),
      'device-recent': new Date(now - 10 * 24 * 60 * 60 * 1000).toISOString(),
    }

    const result = pruneStaleDevices(devices, 90)

    expect(Object.keys(result)).toEqual(['device-recent'])
  })

  it('keeps entries within maxAgeDays', () => {
    const now = Date.now()
    const devices: Record<string, string> = {
      'device-a': new Date(now - 1 * 24 * 60 * 60 * 1000).toISOString(),
      'device-b': new Date(now - 45 * 24 * 60 * 60 * 1000).toISOString(),
      'device-c': new Date(now - 89 * 24 * 60 * 60 * 1000).toISOString(),
    }

    const result = pruneStaleDevices(devices, 90)

    expect(Object.keys(result)).toHaveLength(3)
  })

  it('returns empty object when all entries are stale', () => {
    const now = Date.now()
    const devices: Record<string, string> = {
      'device-a': new Date(now - 100 * 24 * 60 * 60 * 1000).toISOString(),
      'device-b': new Date(now - 200 * 24 * 60 * 60 * 1000).toISOString(),
    }

    const result = pruneStaleDevices(devices, 90)

    expect(Object.keys(result)).toHaveLength(0)
  })

  it('handles empty input', () => {
    const result = pruneStaleDevices({}, 90)
    expect(Object.keys(result)).toHaveLength(0)
  })
})

describe('shouldAlert', () => {
  it('fires when count >= threshold and no previous alert', () => {
    expect(shouldAlert(6, undefined, 6)).toBe(true)
    expect(shouldAlert(10, undefined, 6)).toBe(true)
  })

  it('fires when count >= threshold and last alert was >30 days ago', () => {
    const thirtyOneDaysAgo = new Date(Date.now() - 31 * 24 * 60 * 60 * 1000).toISOString()
    expect(shouldAlert(6, thirtyOneDaysAgo, 6)).toBe(true)
  })

  it('suppressed when last alert was recent', () => {
    const fiveDaysAgo = new Date(Date.now() - 5 * 24 * 60 * 60 * 1000).toISOString()
    expect(shouldAlert(6, fiveDaysAgo, 6)).toBe(false)
    expect(shouldAlert(10, fiveDaysAgo, 6)).toBe(false)
  })

  it('suppressed when count < threshold', () => {
    expect(shouldAlert(5, undefined, 6)).toBe(false)
    expect(shouldAlert(1, undefined, 6)).toBe(false)
    expect(shouldAlert(0, undefined, 6)).toBe(false)
  })

  it('fires at exactly 30 days boundary', () => {
    // At exactly 30 days (not >30), should be suppressed
    vi.useFakeTimers()
    const exactlyThirtyDays = new Date(Date.now() - 30 * 24 * 60 * 60 * 1000).toISOString()
    expect(shouldAlert(6, exactlyThirtyDays, 6)).toBe(false)
    vi.useRealTimers()
  })
})
