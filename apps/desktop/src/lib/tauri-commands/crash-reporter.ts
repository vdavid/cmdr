// Crash reporter commands

import { invoke } from '@tauri-apps/api/core'
import { commands } from '$lib/ipc/bindings'

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
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- excluded from typed bindings (see ipc/CLAUDE.md); tracked for follow-up when specta supports skip_serializing_if
  return invoke<CrashReport | null>('check_pending_crash_report')
}

/** Deletes the crash report without sending. */
export async function dismissCrashReport(): Promise<void> {
  await commands.dismissCrashReport()
}

/** Sends the crash report to the server, then deletes the local file. */
export async function sendCrashReport(report: CrashReport): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- excluded from typed bindings (see ipc/CLAUDE.md); tracked for follow-up when specta supports skip_serializing_if
  await invoke('send_crash_report', { report })
}
