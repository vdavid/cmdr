/**
 * The next-launch crash-report check, run by `routes/(main)/+layout.svelte` once settings load (the auto-send branch
 * reads `updates.crashReports`, so running earlier would read the registry default).
 *
 * With `updates.crashReports` on and no crash loop, the report goes out and a toast says so. Otherwise the dialog
 * decides, which is why the condition is an AND: an app crashing on launch would otherwise send a report every time.
 */
import { checkPendingCrashReport, sendCrashReport, type CrashReport } from '$lib/tauri-commands'
import { getSetting } from '$lib/settings'
import { addToast } from '$lib/ui/toast'
import { getAppLogger } from '$lib/logging/logger'
import { serverRequestFailureOf, serverRequestLogLevel } from '$lib/error-messages/server-request'
import CrashReportToastContent from './CrashReportToastContent.svelte'

const log = getAppLogger('crashReporter')

/** Checks for last session's report, then auto-sends it or hands it to `showDialog`. Never throws. */
export async function checkForPendingCrashReport(showDialog: (report: CrashReport) => void): Promise<void> {
  let report: CrashReport | null
  try {
    report = await checkPendingCrashReport()
  } catch (e) {
    // `check_pending_crash_report` answers an Option and can't refuse, so only a broken IPC bridge lands here.
    // The crash file stays on disk for the next launch.
    log.error('Crash report check returned an error: {error}', { error: String(e) })
    return
  }
  if (!report) return

  if (!getSetting('updates.crashReports') || report.possibleCrashLoop) {
    showDialog(report)
    return
  }

  try {
    await sendCrashReport(report.shortId ?? '')
    addToast(CrashReportToastContent, {
      id: 'crash-report-sent',
      level: 'info',
      dismissal: 'persistent',
      // The toast names the artifact, and "crash report" is only true when the app actually went
      // down with it. `./crash-copy.ts`.
      props: { report },
    })
    log.info('Crash report auto-sent')
  } catch (e) {
    // The file stays until a send lands, so the report comes back next launch. No network, a timeout, or a
    // server having a bad moment stays at warn; a refusal from Cmdr's own server means the contract broke.
    const detail = { error: String(e) }
    const level = serverRequestLogLevel(serverRequestFailureOf(e))
    if (level === 'error') {
      log.error('Auto-send crash report returned an error: {error}', detail)
    } else if (level === 'warn') {
      log.warn('Auto-send crash report returned an error: {error}', detail)
    } else {
      log.info('Auto-send crash report not sent: {error}', detail)
    }
  }
}
