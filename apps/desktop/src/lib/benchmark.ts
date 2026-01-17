/**
 * Performance timeline for file loading benchmarks.
 *
 * Enable with RUSTY_COMMANDER_BENCHMARK=1 (Rust) and VITE_BENCHMARK=1 (TS).
 * Both are needed for a complete unified timeline.
 *
 * FE events are sent to Rust via IPC to appear in the same stderr stream
 * as Rust events, creating a single chronologically interleaved timeline.
 *
 * @module benchmark
 */

import { invoke } from '@tauri-apps/api/core'

/* eslint-disable no-console */

// Check if benchmarking is enabled
const isBenchmarkEnabled = (): boolean => {
    // Check for environment variable (set at build time)
    if (import.meta.env.VITE_BENCHMARK === '1' || import.meta.env.VITE_BENCHMARK === 'true') {
        return true
    }
    // Also check for runtime flag (useful for testing)
    // noinspection PointlessBooleanExpressionJS - It can't be simplified.
    return typeof window !== 'undefined' && (window as unknown as { __BENCHMARK__?: boolean }).__BENCHMARK__ === true
}

// Epoch for relative timestamps (reset per navigation)
let epochMs = 0

/**
 * Send a log message to Rust's stderr (and also to console for debugging)
 */
function sendToRust(message: string): void {
    console.log(message) // Also log locally for DevTools
    // Fire-and-forget: don't await, we don't want to slow down the app
    void invoke('benchmark_log', { message }).catch(() => {
        // Ignore errors - benchmarking should never break the app
    })
}

/**
 * Reset the epoch (call when user navigates to a folder)
 */
export function resetEpoch(): void {
    epochMs = performance.now()
    if (isBenchmarkEnabled()) {
        sendToRust('[TIMELINE]          0μs | FE   | EPOCH_RESET')
    }
}

/**
 * Get current timestamp in microseconds since epoch
 */
function nowMicros(): number {
    return Math.round((performance.now() - epochMs) * 1000)
}

/**
 * Log a benchmark event
 */
export function logEvent(event: string): void {
    if (!isBenchmarkEnabled()) return
    const ts = nowMicros()
    sendToRust(`[TIMELINE] ${String(ts).padStart(10)}μs | FE   | ${event}`)
}

/**
 * Log a benchmark event with a value
 */
export function logEventValue(event: string, value: unknown): void {
    if (!isBenchmarkEnabled()) return
    const ts = nowMicros()
    sendToRust(`[TIMELINE] ${String(ts).padStart(10)}μs | FE   | ${event} = ${String(value)}`)
}
