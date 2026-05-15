import { describe, it, expect } from 'vitest'
import { formatFilesPerSecond } from './write-operations'

describe('formatFilesPerSecond', () => {
  describe('rates below 3 (1 decimal)', () => {
    it('formats sub-1 rates with 1 decimal', () => {
      expect(formatFilesPerSecond(0.4)).toBe('0.4 files/s')
    })

    it('formats 1.x rates with 1 decimal (plural)', () => {
      expect(formatFilesPerSecond(1.8)).toBe('1.8 files/s')
    })

    it('formats 2.x rates with 1 decimal', () => {
      expect(formatFilesPerSecond(2.5)).toBe('2.5 files/s')
    })

    it('rounds to 1 decimal', () => {
      expect(formatFilesPerSecond(0.44)).toBe('0.4 files/s')
      expect(formatFilesPerSecond(0.45)).toBe('0.5 files/s')
    })
  })

  describe('singular ("1 file/s")', () => {
    it('uses singular when rate is exactly 1', () => {
      expect(formatFilesPerSecond(1)).toBe('1 file/s')
    })

    it('uses singular when rate rounds to 1.0 from below', () => {
      expect(formatFilesPerSecond(0.97)).toBe('1 file/s')
    })

    it('uses singular when rate rounds to 1.0 from above', () => {
      expect(formatFilesPerSecond(1.04)).toBe('1 file/s')
    })

    it('switches to plural at 1.05', () => {
      expect(formatFilesPerSecond(1.05)).toBe('1.1 files/s')
    })
  })

  describe('rates at or above 3 (integer)', () => {
    it('rounds to integer at exactly 3', () => {
      expect(formatFilesPerSecond(3)).toBe('3 files/s')
    })

    it('rounds 27.4 down to 27', () => {
      expect(formatFilesPerSecond(27.4)).toBe('27 files/s')
    })

    it('rounds 27.5 up to 28', () => {
      expect(formatFilesPerSecond(27.5)).toBe('28 files/s')
    })

    it('handles large rates', () => {
      expect(formatFilesPerSecond(1500)).toBe('1500 files/s')
    })
  })

  describe('returns null when rate rounds to 0', () => {
    it('returns null for exactly 0', () => {
      expect(formatFilesPerSecond(0)).toBe(null)
    })

    it('returns null for rates < 0.05 (round to 0.0)', () => {
      expect(formatFilesPerSecond(0.04)).toBe(null)
      expect(formatFilesPerSecond(0.0001)).toBe(null)
    })

    it('returns "0.1 files/s" at the 0.05 boundary', () => {
      expect(formatFilesPerSecond(0.05)).toBe('0.1 files/s')
    })
  })
})
