// Crash reporter commands

import { invoke } from '@tauri-apps/api/core'

/** Crash report data from the backend. */
export interface CrashReport {
    version: number
    timestamp: string
    signal: string | null
    panicMessage: string | null
    backtraceFrames: string[]
    threadName: string | null
    threadCount: number
    appVersion: string
    osVersion: string
    arch: string
    uptimeSecs: number
    activeSettings: {
        indexingEnabled: boolean | null
        aiProvider: string | null
        mcpEnabled: boolean | null
        verboseLogging: boolean | null
    }
    possibleCrashLoop: boolean
}

/** Checks for a pending crash report from a previous session. */
export async function checkPendingCrashReport(): Promise<CrashReport | null> {
    return invoke<CrashReport | null>('check_pending_crash_report')
}

/** Deletes the crash report without sending. */
export async function dismissCrashReport(): Promise<void> {
    await invoke('dismiss_crash_report')
}

/** Sends the crash report to the server, then deletes the local file. */
export async function sendCrashReport(report: CrashReport): Promise<void> {
    await invoke('send_crash_report', { report })
}
