/**
 * Unit tests for `toast-age.ts`: the "2m ago" label a toast wears once it's a minute old,
 * and when that label next changes.
 */

import { describe, it, expect } from 'vitest'
import { formatToastAge, msUntilToastAgeChanges } from './toast-age'

const SECOND = 1000
const MINUTE = 60 * SECOND
const HOUR = 60 * MINUTE

describe('formatToastAge', () => {
  it('shows nothing while the toast is under a minute old', () => {
    expect(formatToastAge(0)).toBeNull()
    expect(formatToastAge(59 * SECOND)).toBeNull()
  })

  it('treats a negative age (a clock read before the toast was posted) as brand new', () => {
    expect(formatToastAge(-5 * SECOND)).toBeNull()
  })

  it('counts whole minutes from one minute up', () => {
    expect(formatToastAge(MINUTE)).toBe('1m ago')
    expect(formatToastAge(2 * MINUTE + 59 * SECOND)).toBe('2m ago')
    expect(formatToastAge(59 * MINUTE + 59 * SECOND)).toBe('59m ago')
  })

  it('switches to whole hours from one hour up', () => {
    expect(formatToastAge(HOUR)).toBe('1h ago')
    expect(formatToastAge(3 * HOUR + 42 * MINUTE)).toBe('3h ago')
  })
})

describe('msUntilToastAgeChanges', () => {
  it('waits out the first minute on a fresh toast', () => {
    expect(msUntilToastAgeChanges(0)).toBe(MINUTE)
    expect(msUntilToastAgeChanges(45 * SECOND)).toBe(15 * SECOND)
  })

  it('wakes on the next whole minute under an hour', () => {
    expect(msUntilToastAgeChanges(2 * MINUTE + 10 * SECOND)).toBe(50 * SECOND)
  })

  it('wakes on the next whole hour past an hour, not every minute', () => {
    expect(msUntilToastAgeChanges(HOUR + 10 * MINUTE)).toBe(50 * MINUTE)
  })

  it('counts a negative age as time still to wait', () => {
    expect(msUntilToastAgeChanges(-5 * SECOND)).toBe(MINUTE + 5 * SECOND)
  })
})
