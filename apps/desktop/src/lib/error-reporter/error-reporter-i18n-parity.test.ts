/**
 * Base-locale (en) parity net for the error-reporter i18n migration.
 *
 * The error-report dialog, its post-send / bundle-saved / auto-sent toasts, and
 * the dialog's failure toasts moved from hardcoded English into the
 * `errorReporter.*` catalog (resolved through `t()`). This is a
 * behavior-preserving MOVE: every rendered en string must be byte-identical to
 * the pre-migration copy. These goldens are the literals that lived in
 * `ErrorReportDialog.svelte` and the toast components before the move; an
 * intended copy edit lands in the catalog AND here together, never silently.
 *
 * The plural sample-line headings replace the old `pluralize(n, 'line')` call:
 * `one` for 1, `other` for 0 and 2+, matching `pluralize` exactly.
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

describe('error-reporter dialog copy parity (en)', () => {
  it('resolves the static dialog strings', () => {
    expect(tString('errorReporter.dialog.title')).toBe('Send error report')
    expect(tString('errorReporter.dialog.description')).toBe(
      "This sends Cmdr's recent log files to the team so we can fix what went wrong. The logs are redacted client-side: file paths, hostnames, IPs, and emails are all scrubbed before sending.",
    )
    expect(tString('errorReporter.dialog.referenceIdLabel')).toBe('Reference ID:')
    expect(tString('errorReporter.dialog.copy')).toBe('Copy')
    expect(tString('errorReporter.dialog.copied')).toBe('Copied')
    expect(tString('errorReporter.dialog.noteLabel')).toBe('Add a note (optional)')
    expect(tString('errorReporter.dialog.notePlaceholder')).toBe(
      'What were you trying to do? What did you expect to happen?',
    )
    expect(tString('errorReporter.dialog.detailsToggle')).toBe("What's about to be sent")
    expect(tString('errorReporter.dialog.manifestHeading')).toBe('Manifest')
    expect(tString('errorReporter.dialog.noLogLines')).toBe('(no log lines available)')
    expect(tString('errorReporter.dialog.preparing')).toBe('Preparing preview…')
    expect(tString('errorReporter.dialog.saveToDisk')).toBe('Save bundle to disk (debug)')
    expect(tString('errorReporter.dialog.cancel')).toBe('Cancel')
    expect(tString('errorReporter.dialog.send')).toBe('Send report')
    expect(tString('errorReporter.dialog.sending')).toBe('Sending…')
  })

  it('resolves the interpolated dialog strings', () => {
    expect(t('errorReporter.dialog.counter', { currentText: '52,000', maxText: '100,000' })).toBe('52,000 / 100,000')
    expect(t('errorReporter.dialog.noteTooLong', { maxText: '100,000' })).toBe(
      'Note is too long. Maximum is 100,000 characters.',
    )
    expect(t('errorReporter.dialog.totalLines', { countText: '1,234' })).toBe(
      'Total log lines (after redaction): 1,234',
    )
    expect(t('errorReporter.dialog.prepareFailed', { error: 'boom' })).toBe("Couldn't prepare preview: boom")
    expect(t('errorReporter.dialog.sendFailedToast', { error: 'boom' })).toBe("Couldn't send error report: boom")
    expect(t('errorReporter.dialog.saveFailedToast', { error: 'boom' })).toBe("Couldn't save bundle: boom")
  })

  it('matches the old pluralize(n, "line") for the sample headings', () => {
    expect(t('errorReporter.dialog.sampleFirstHeading', { count: 1 })).toBe('Sample of first 1 line')
    expect(t('errorReporter.dialog.sampleFirstHeading', { count: 0 })).toBe('Sample of first 0 lines')
    expect(t('errorReporter.dialog.sampleFirstHeading', { count: 5 })).toBe('Sample of first 5 lines')
    expect(t('errorReporter.dialog.sampleLastHeading', { count: 1 })).toBe('Sample of last 1 line')
    expect(t('errorReporter.dialog.sampleLastHeading', { count: 3 })).toBe('Sample of last 3 lines')
  })
})

describe('error-reporter toast copy parity (en)', () => {
  it('resolves the post-send toast', () => {
    expect(tString('errorReporter.sentToast.message')).toBe('Error report sent. Your reference ID is')
    expect(tString('errorReporter.sentToast.dismiss')).toBe('Dismiss')
    expect(tString('errorReporter.sentToast.copyId')).toBe('Copy ID')
    expect(tString('errorReporter.sentToast.copied')).toBe('Copied')
  })

  it('resolves the bundle-saved toast', () => {
    expect(tString('errorReporter.bundleSavedToast.message')).toBe('Saved bundle to disk')
    expect(tString('errorReporter.bundleSavedToast.dismiss')).toBe('Dismiss')
    expect(tString('errorReporter.bundleSavedToast.reveal')).toBe('Reveal in Finder')
  })

  it('resolves the auto-sent toast', () => {
    expect(tString('errorReporter.autoSentToast.title')).toBe('Error report sent')
    expect(tString('errorReporter.autoSentToast.referenceIdLabel')).toBe('Reference ID:')
    expect(tString('errorReporter.autoSentToast.changeSettings')).toBe('Change settings')
    expect(tString('errorReporter.autoSentToast.viewOrAddNotes')).toBe('View or add notes to the report')
  })

  it('resolves the amended-report toast', () => {
    expect(tString('errorReporter.amendedToast.message')).toBe('Note added to your report. Your reference ID is')
  })
})

/**
 * Amend mode is new copy, not a move, so these goldens exist to make an edit
 * deliberate: changing one here means changing it in every locale too.
 */
describe('error-reporter amend-mode copy (en)', () => {
  it('resolves the static amend strings', () => {
    expect(tString('errorReporter.amend.title')).toBe('Add to your error report')
    expect(tString('errorReporter.amend.description')).toBe(
      "Cmdr already sent this report. Write a note, or attach your email, and it'll join what the team already has.",
    )
    expect(tString('errorReporter.amend.noteLabel')).toBe('Your note')
    expect(tString('errorReporter.amend.detailsToggle')).toBe('What was sent')
    expect(tString('errorReporter.amend.submit')).toBe('Add to report')
    expect(tString('errorReporter.amend.submitting')).toBe('Adding…')
    expect(tString('errorReporter.amend.close')).toBe('Close')
    expect(tString('errorReporter.amend.unavailable')).toBe(
      "That report can't take a note any more. To get your notes to the team, send a new report from the Help menu.",
    )
  })

  it('resolves the interpolated amend strings', () => {
    expect(t('errorReporter.amend.addFailedToast', { error: 'boom' })).toBe("Couldn't add your note: boom")
  })
})
