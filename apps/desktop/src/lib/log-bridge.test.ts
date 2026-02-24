import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { LogRecord } from '@logtape/logtape'

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(() => Promise.resolve()),
}))

import { invoke } from '@tauri-apps/api/core'
import { getTauriBridgeSink, stopBridge } from './log-bridge'

function makeRecord(overrides: Partial<LogRecord> = {}): LogRecord {
    return {
        level: 'info',
        category: ['app', 'test'],
        message: ['test message'],
        rawMessage: 'test message',
        properties: {},
        timestamp: Date.now(),
        ...overrides,
    } as LogRecord
}

describe('log-bridge', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.clearAllMocks()
        stopBridge()
    })

    afterEach(() => {
        stopBridge()
        vi.useRealTimers()
    })

    it('batches entries and flushes after 100ms', async () => {
        const sink = getTauriBridgeSink()
        sink(makeRecord())

        expect(invoke).not.toHaveBeenCalled()

        await vi.advanceTimersByTimeAsync(100)

        expect(invoke).toHaveBeenCalledOnce()
        expect(invoke).toHaveBeenCalledWith('batch_fe_logs', {
            entries: [{ level: 'info', category: 'test', message: 'test message' }],
        })
    })

    it('deduplicates identical consecutive messages', async () => {
        const sink = getTauriBridgeSink()
        const record = makeRecord()

        for (let i = 0; i < 5; i++) {
            sink(record)
        }

        await vi.advanceTimersByTimeAsync(100)

        expect(invoke).toHaveBeenCalledOnce()
        expect(invoke).toHaveBeenCalledWith('batch_fe_logs', {
            entries: [{ level: 'info', category: 'test', message: 'test message (×5, deduplicated)' }],
        })
    })

    it('does not deduplicate non-consecutive identical messages', async () => {
        const sink = getTauriBridgeSink()
        const recordA = makeRecord({ message: ['message A'] })
        const recordB = makeRecord({ message: ['message B'] })

        sink(recordA)
        sink(recordB)
        sink(recordA)

        await vi.advanceTimersByTimeAsync(100)

        expect(invoke).toHaveBeenCalledOnce()
        expect(invoke).toHaveBeenCalledWith('batch_fe_logs', {
            entries: [
                { level: 'info', category: 'test', message: 'message A' },
                { level: 'info', category: 'test', message: 'message B' },
                { level: 'info', category: 'test', message: 'message A' },
            ],
        })
    })

    it('throttles at 200/s and emits warning', async () => {
        const sink = getTauriBridgeSink()

        for (let i = 0; i < 210; i++) {
            sink(makeRecord({ message: [`msg ${String(i)}`] }))
        }

        await vi.advanceTimersByTimeAsync(100)

        expect(invoke).toHaveBeenCalledOnce()

        const callArgs = vi.mocked(invoke).mock.calls[0] as [
            string,
            { entries: { level: string; category: string; message: string }[] },
        ]
        const entries = callArgs[1].entries

        // 200 regular entries + 1 throttle warning
        expect(entries).toHaveLength(201)

        // The warning entry should mention dropped entries
        const warningEntry = entries.find((e) => e.category === 'log-bridge')
        expect(warningEntry).toBeDefined()
        expect(warningEntry?.level).toBe('warn')
        expect(warningEntry?.message).toContain('entries dropped in the last second')
    })

    it('sends nothing when buffer is empty', async () => {
        await vi.advanceTimersByTimeAsync(200)
        expect(invoke).not.toHaveBeenCalled()
    })

    it('maps LogTape "warning" level to "warn"', async () => {
        const sink = getTauriBridgeSink()
        sink(makeRecord({ level: 'warning' }))

        await vi.advanceTimersByTimeAsync(100)

        expect(invoke).toHaveBeenCalledWith('batch_fe_logs', {
            entries: [{ level: 'warn', category: 'test', message: 'test message' }],
        })
    })

    it('strips "app" prefix from category', async () => {
        const sink = getTauriBridgeSink()
        sink(makeRecord({ category: ['app', 'fileExplorer'] }))

        await vi.advanceTimersByTimeAsync(100)

        const callArgs = vi.mocked(invoke).mock.calls[0] as [string, { entries: { category: string }[] }]
        expect(callArgs[1].entries[0].category).toBe('fileExplorer')
    })

    it('joins multi-part message arrays into a string', async () => {
        const sink = getTauriBridgeSink()
        sink(makeRecord({ message: ['Loaded ', 42, ' items'] }))

        await vi.advanceTimersByTimeAsync(100)

        const callArgs = vi.mocked(invoke).mock.calls[0] as [string, { entries: { message: string }[] }]
        expect(callArgs[1].entries[0].message).toBe('Loaded 42 items')
    })

    it('silently drops entries when invoke fails', async () => {
        vi.mocked(invoke).mockRejectedValueOnce(new Error('Backend unavailable'))

        const sink = getTauriBridgeSink()
        sink(makeRecord())

        // Should not throw
        await vi.advanceTimersByTimeAsync(100)
    })
})
