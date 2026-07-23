import { describe, it, expect } from 'vitest'
import { durationUnitFactor, msToDurationValue, durationValueToMs } from './types'

describe('durationUnitFactor', () => {
    it('maps each unit to its millisecond factor', () => {
        expect(durationUnitFactor('ms')).toBe(1)
        expect(durationUnitFactor('s')).toBe(1000)
        expect(durationUnitFactor('min')).toBe(60_000)
        expect(durationUnitFactor('h')).toBe(3_600_000)
        expect(durationUnitFactor('d')).toBe(86_400_000)
    })

    it('treats a missing unit as a raw (factor 1) number', () => {
        expect(durationUnitFactor(undefined)).toBe(1)
    })
})

describe('ms <-> display value', () => {
    it('converts stored ms into the display unit', () => {
        expect(msToDurationValue(20_000, 's')).toBe(20)
        expect(msToDurationValue(3_600_000, 'min')).toBe(60)
        expect(msToDurationValue(200, 'ms')).toBe(200)
    })

    it('converts a display value back into stored ms', () => {
        expect(durationValueToMs(20, 's')).toBe(20_000)
        expect(durationValueToMs(60, 'min')).toBe(3_600_000)
        expect(durationValueToMs(200, 'ms')).toBe(200)
    })

    it('round-trips every advanced duration default exactly', () => {
        // The real registry defaults (advanced.ts): value chosen so ms is a clean
        // multiple of the unit factor, so display <-> stored is lossless.
        const cases: { ms: number; unit: 's' | 'ms' | 'min' }[] = [
            { ms: 200, unit: 'ms' }, // fileWatcherDebounce
            { ms: 20_000, unit: 's' }, // mountTimeout
            { ms: 5_000, unit: 's' }, // serviceResolveTimeout
            { ms: 3_600_000, unit: 'min' }, // updateCheckInterval
        ]
        for (const { ms, unit } of cases) {
            expect(durationValueToMs(msToDurationValue(ms, unit), unit)).toBe(ms)
        }
    })

    it('treats an undefined unit as a passthrough (plain number)', () => {
        expect(msToDurationValue(42, undefined)).toBe(42)
        expect(durationValueToMs(42, undefined)).toBe(42)
    })
})
