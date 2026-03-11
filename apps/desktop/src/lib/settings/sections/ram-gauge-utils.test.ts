import { describe, it, expect } from 'vitest'
import { computeGaugeSegments, type GaugeSegments } from './ram-gauge-utils'
import type { SystemMemoryInfo } from '$lib/tauri-commands'

const GB = 1024 * 1024 * 1024

/** Helper: builds a SystemMemoryInfo where segments sum to total. */
function mem(totalGb: number, wiredGb: number, appGb: number, freeGb: number): SystemMemoryInfo {
    return { totalBytes: totalGb * GB, wiredBytes: wiredGb * GB, appBytes: appGb * GB, freeBytes: freeGb * GB }
}

/** Asserts the result is non-null and returns it typed. */
function expectSegments(memory: SystemMemoryInfo, currentAi: number, projectedAi: number): GaugeSegments {
    const result = computeGaugeSegments(memory, currentAi, projectedAi)
    expect(result).not.toBeNull()
    return result as GaugeSegments
}

describe('computeGaugeSegments', () => {
    it('returns null when total is 0', () => {
        expect(computeGaugeSegments(mem(0, 0, 0, 0), 0, 0)).toBeNull()
    })

    it('segments add up to <= 100%', () => {
        const result = expectSegments(mem(64, 5, 30, 29), 3.5 * GB, 3.5 * GB)
        const sum =
            result.systemPercent +
            result.otherAppsPercent +
            result.retainedAiPercent +
            result.addedPercent +
            result.freedPercent
        expect(sum).toBeLessThanOrEqual(100.01) // floating point tolerance
        expect(sum).toBeGreaterThan(0)
    })

    it('system segment uses wired bytes (non-zero for real systems)', () => {
        const result = expectSegments(mem(64, 5, 30, 29), 3.5 * GB, 3.5 * GB)
        expect(result.systemBytes).toBe(5 * GB)
        expect(result.systemPercent).toBeCloseTo((5 / 64) * 100, 1)
    })

    it('other apps = app memory minus AI memory', () => {
        const result = expectSegments(mem(64, 5, 30, 29), 3.5 * GB, 3.5 * GB)
        expect(result.otherAppsBytes).toBe(26.5 * GB)
    })

    it('free bytes come directly from system memory', () => {
        const result = expectSegments(mem(64, 5, 30, 29), 3.5 * GB, 3.5 * GB)
        expect(result.freeBytes).toBe(29 * GB)
    })

    it('other apps bytes are clamped to 0 when AI estimate exceeds app memory', () => {
        // AI estimate larger than reported app memory (edge case during model load)
        const result = expectSegments(mem(64, 5, 2, 57), 3.5 * GB, 3.5 * GB)
        expect(result.otherAppsBytes).toBe(0)
    })

    it('shows added segment when projected > current (growing)', () => {
        const current = 2 * GB
        const projected = 4 * GB
        const result = expectSegments(mem(64, 5, 30, 29), current, projected)
        expect(result.addedPercent).toBeGreaterThan(0)
        expect(result.freedPercent).toBe(0)
        expect(result.retainedAiPercent).toBeCloseTo((2 / 64) * 100, 1)
        expect(result.addedPercent).toBeCloseTo((2 / 64) * 100, 1)
    })

    it('shows freed segment when projected < current (shrinking)', () => {
        const current = 4 * GB
        const projected = 2 * GB
        const result = expectSegments(mem(64, 5, 30, 29), current, projected)
        expect(result.freedPercent).toBeGreaterThan(0)
        expect(result.addedPercent).toBe(0)
        expect(result.retainedAiPercent).toBeCloseTo((2 / 64) * 100, 1)
        expect(result.freedPercent).toBeCloseTo((2 / 64) * 100, 1)
    })

    it('no change segments when projected == current', () => {
        const result = expectSegments(mem(64, 5, 30, 29), 3 * GB, 3 * GB)
        expect(result.addedPercent).toBe(0)
        expect(result.freedPercent).toBe(0)
    })

    it('clamps to 100% when segments would overflow', () => {
        // Extreme case: all memory categories are huge relative to total
        const result = expectSegments(mem(16, 4, 10, 2), 6 * GB, 10 * GB)
        const sum =
            result.systemPercent +
            result.otherAppsPercent +
            result.retainedAiPercent +
            result.addedPercent +
            result.freedPercent
        expect(sum).toBeLessThanOrEqual(100.01)
    })

    it('totalProjectedUsageRatio reflects projected AI, not current', () => {
        const current = 2 * GB
        const projected = 6 * GB
        const result = expectSegments(mem(64, 5, 30, 29), current, projected)
        // system(5) + otherApps(30-2=28) + projected(6) = 39 / 64 ≈ 0.609
        expect(result.totalProjectedUsageRatio).toBeCloseTo(39 / 64, 2)
    })

    it('AI server not running (0 current) shows only projected as added', () => {
        const result = expectSegments(mem(64, 5, 30, 29), 0, 3.5 * GB)
        expect(result.retainedAiPercent).toBe(0)
        expect(result.addedPercent).toBeCloseTo((3.5 / 64) * 100, 1)
        // Other apps = full app memory since AI current is 0
        expect(result.otherAppsBytes).toBe(30 * GB)
    })
})
