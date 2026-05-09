// Crash reporter commands

import { commands } from '$lib/ipc/bindings'
import type { CrashReport } from '$lib/ipc/bindings'

export type { CrashReport }

/** Checks for a pending crash report from a previous session. */
export async function checkPendingCrashReport(): Promise<CrashReport | null> {
  return commands.checkPendingCrashReport()
}

/** Deletes the crash report without sending. */
export async function dismissCrashReport(): Promise<void> {
  await commands.dismissCrashReport()
}

/** Sends the crash report to the server, then deletes the local file. */
export async function sendCrashReport(report: CrashReport): Promise<void> {
  const result = await commands.sendCrashReport(report)
  if (result.status === 'error') throw new Error(result.error)
}
