/**
 * Batching log bridge that sends frontend logs to the Rust backend.
 *
 * Handles three concerns:
 * 1. Batching: collects entries for 100ms, sends in one IPC call
 * 2. Deduplication: collapses identical messages within a batch window
 * 3. Throttling: caps at 200 entries/second, warns on excess
 */

import { invoke } from '@tauri-apps/api/core'
import type { LogRecord, Sink } from '@logtape/logtape'

interface PendingEntry {
    level: string
    category: string
    message: string
    count: number
}

const BATCH_INTERVAL_MS = 100
const MAX_ENTRIES_PER_SECOND = 200

let pendingEntries: PendingEntry[] = []
let batchTimer: ReturnType<typeof setTimeout> | null = null
let entriesThisSecond = 0
let throttleResetTimer: ReturnType<typeof setInterval> | null = null
let throttleWarningEmitted = false
let droppedCount = 0

function formatMessage(record: LogRecord): string {
    // LogTape message is an array of interleaved template parts and values,
    // for example ["Loading ", 42, " items"]. Join them into a single string.
    return record.message.map(String).join('')
}

function getCategory(record: LogRecord): string {
    // LogTape categories are arrays like ['app', 'fileExplorer']
    // Skip the 'app' root prefix, join the rest
    const parts = record.category.length > 1 ? record.category.slice(1) : record.category
    return parts.join('.')
}

function mapLevel(level: string): string {
    // LogTape uses "warning", Rust log uses "warn"
    if (level === 'warning') return 'warn'
    return level
}

function addEntry(level: string, category: string, message: string): void {
    // Check throttle
    if (entriesThisSecond >= MAX_ENTRIES_PER_SECOND) {
        droppedCount++
        if (!throttleWarningEmitted) {
            throttleWarningEmitted = true
            // Schedule the warning to be sent at the next flush
            pendingEntries.push({
                level: 'warn',
                category: 'log-bridge',
                message: `Excessive frontend logging detected: entries are being dropped (>${String(MAX_ENTRIES_PER_SECOND)}/s). This may indicate a bug (infinite loop, runaway effect).`,
                count: 1,
            })
        }
        return
    }

    entriesThisSecond++

    // Deduplication: check if last entry in pending batch is identical
    const last = pendingEntries.at(-1)
    if (last && last.level === level && last.category === category && last.message === message) {
        last.count++
        return
    }

    pendingEntries.push({ level, category, message, count: 1 })
    scheduleBatch()
}

function scheduleBatch(): void {
    if (batchTimer !== null) return
    batchTimer = setTimeout(() => {
        void flush()
    }, BATCH_INTERVAL_MS)
}

async function flush(): Promise<void> {
    batchTimer = null
    if (pendingEntries.length === 0) return

    const entries = pendingEntries
    pendingEntries = []

    // Update the throttle warning with the actual dropped count
    if (droppedCount > 0) {
        const warningIdx = entries.findIndex((e) => e.category === 'log-bridge' && e.level === 'warn')
        if (warningIdx >= 0) {
            entries[warningIdx].message =
                `Excessive frontend logging detected: ${String(droppedCount)} entries dropped in the last second. This may indicate a bug (infinite loop, runaway effect).`
        }
        droppedCount = 0
        throttleWarningEmitted = false
    }

    // Format entries for IPC
    const ipcEntries = entries.map((e) => ({
        level: e.level,
        category: e.category,
        message: e.count > 1 ? `${e.message} (×${String(e.count)}, deduplicated)` : e.message,
    }))

    try {
        await invoke('batch_fe_logs', { entries: ipcEntries })
    } catch {
        // Backend not available (app shutting down, or early startup) — silently drop
    }
}

/** LogTape sink that batches and sends logs to the Rust backend. */
export function getTauriBridgeSink(): Sink {
    return (record: LogRecord): void => {
        const level = mapLevel(record.level)
        const category = getCategory(record)
        const message = formatMessage(record)
        addEntry(level, category, message)
    }
}

/** Start the per-second throttle reset timer. Call once at init. */
export function startBridge(): void {
    if (throttleResetTimer !== null) return
    throttleResetTimer = setInterval(() => {
        entriesThisSecond = 0
    }, 1000)

    // Flush remaining logs when the page unloads
    window.addEventListener('beforeunload', () => {
        void flush()
    })
}

/** Stop the bridge (for cleanup in tests). */
export function stopBridge(): void {
    if (throttleResetTimer !== null) {
        clearInterval(throttleResetTimer)
        throttleResetTimer = null
    }
    if (batchTimer !== null) {
        clearTimeout(batchTimer)
        batchTimer = null
    }
    pendingEntries = []
    entriesThisSecond = 0
    droppedCount = 0
    throttleWarningEmitted = false
}
