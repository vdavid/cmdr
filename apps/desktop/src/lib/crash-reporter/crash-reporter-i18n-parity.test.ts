/**
 * Base-locale (en) parity net for the crash-reporter i18n migration.
 *
 * The crash-report dialog and its sent-confirmation toast copy moved from
 * hardcoded English into the `crashReporter.*` catalog (resolved through `t()`).
 * This is a behavior-preserving MOVE: every rendered en string must be
 * byte-identical to the pre-migration copy. These goldens are the literals that
 * lived in `CrashReportDialog.svelte` and `CrashReportToastContent.svelte`
 * before the move; an intended copy edit lands in the catalog AND here together,
 * never silently.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { t, tString } from '$lib/intl/messages.svelte'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('crash-reporter dialog copy parity (en)', () => {
  it('resolves the static dialog strings', () => {
    expect(tString('crashReporter.dialog.title')).toBe('Send crash report?')
    expect(tString('crashReporter.dialog.body')).toBe(
      "Cmdr quit unexpectedly last time. Here's a crash report with details that can help fix this.",
    )
    expect(tString('crashReporter.dialog.privacyNote')).toBe(
      'It includes the app version, macOS version, and which part of the code crashed. No file names or personal data.',
    )
    expect(tString('crashReporter.dialog.reportIdLabel')).toBe('Report ID:')
    expect(tString('crashReporter.dialog.reportIdHelp')).toBe('Mention this if you reach out about the issue.')
    expect(tString('crashReporter.dialog.showDetails')).toBe('Show report details')
    expect(tString('crashReporter.dialog.copy')).toBe('Copy')
    expect(tString('crashReporter.dialog.copied')).toBe('Copied')
    expect(tString('crashReporter.dialog.alwaysSend')).toBe('Always send crash reports')
    expect(tString('crashReporter.dialog.dismiss')).toBe('Dismiss')
    expect(tString('crashReporter.dialog.send')).toBe('Send report')
    expect(tString('crashReporter.dialog.sending')).toBe('Sending...')
  })

  it('resolves the interpolated attach-email label', () => {
    expect(t('crashReporter.dialog.attachEmail', { email: 'alex@example.com' })).toBe(
      'Attach my email (alex@example.com) so we can reply',
    )
  })
})

describe('crash-reporter sent-toast copy parity (en)', () => {
  it('resolves the toast strings', () => {
    expect(tString('crashReporter.sentToast.message')).toBe('Crash report sent. Thanks for helping improve Cmdr.')
    expect(tString('crashReporter.sentToast.changeSettings')).toBe('Change in Settings > Updates')
  })
})
