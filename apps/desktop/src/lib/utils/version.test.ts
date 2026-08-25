import { describe, it, expect } from 'vitest'
import { compareVersions } from './version'

describe('compareVersions', () => {
  it('orders by major, minor, then patch', () => {
    expect(compareVersions('0.25.0', '0.26.0')).toBeLessThan(0)
    expect(compareVersions('1.0.0', '0.99.99')).toBeGreaterThan(0)
    expect(compareVersions('0.25.3', '0.25.3')).toBe(0)
  })

  it('orders double-digit components numerically, not lexically', () => {
    // The classic trap: a string compare puts "0.10.0" before "0.9.0".
    expect(compareVersions('0.9.0', '0.10.0')).toBeLessThan(0)
    expect(compareVersions('0.10.0', '0.9.0')).toBeGreaterThan(0)
    expect(compareVersions('1.2.10', '1.2.9')).toBeGreaterThan(0)
  })

  it('tolerates a leading v and pre-release / build suffixes (compared by core only)', () => {
    expect(compareVersions('v0.25.0', '0.25.0')).toBe(0)
    expect(compareVersions('0.26.0-beta.1', '0.26.0')).toBe(0)
  })

  it('reads a release two minors ahead of a staged one as newer', () => {
    // The updater's re-check question: 0.33.0 shipped while 0.29.0 sat staged in the bundle.
    expect(compareVersions('0.33.0', '0.29.0')).toBeGreaterThan(0)
    expect(compareVersions('0.29.0', '0.29.0')).toBe(0)
  })
})
