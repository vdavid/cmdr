/**
 * Tests for the error-report flow store.
 *
 * The store has two operations: open (with optional initial note) and close.
 * Both menu items and toast buttons funnel through `openErrorReportDialog`,
 * so this is the right place to assert the open/close contract.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import {
  errorReportFlow,
  openErrorReportDialog,
  openErrorReportDialogForAutoSentReport,
  closeErrorReportDialog,
} from './error-report-flow.svelte'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve()),
}))

beforeEach(() => {
  closeErrorReportDialog()
})

describe('error-report-flow', () => {
  it('starts closed with an empty initial note', () => {
    expect(errorReportFlow.open).toBe(false)
    expect(errorReportFlow.initialNote).toBe('')
  })

  it('openErrorReportDialog flips open to true', () => {
    openErrorReportDialog()
    expect(errorReportFlow.open).toBe(true)
  })

  it('openErrorReportDialog stores the initial note when provided', () => {
    openErrorReportDialog('something broke')
    expect(errorReportFlow.open).toBe(true)
    expect(errorReportFlow.initialNote).toBe('something broke')
  })

  it('openErrorReportDialog defaults the initial note to empty string when omitted', () => {
    openErrorReportDialog()
    expect(errorReportFlow.initialNote).toBe('')
  })

  it('closeErrorReportDialog resets both fields', () => {
    openErrorReportDialog('lingering note')
    closeErrorReportDialog()
    expect(errorReportFlow.open).toBe(false)
    expect(errorReportFlow.initialNote).toBe('')
  })

  it('reopening replaces the initial note', () => {
    openErrorReportDialog('first note')
    closeErrorReportDialog()
    openErrorReportDialog('second note')
    expect(errorReportFlow.initialNote).toBe('second note')
  })

  it('starts in compose mode', () => {
    expect(errorReportFlow.mode).toBe('compose')
    openErrorReportDialog()
    expect(errorReportFlow.mode).toBe('compose')
  })

  it('openErrorReportDialogForAutoSentReport opens in amend mode', () => {
    openErrorReportDialogForAutoSentReport()
    expect(errorReportFlow.open).toBe(true)
    expect(errorReportFlow.mode).toBe('amend')
  })

  it('closeErrorReportDialog drops amend mode', () => {
    openErrorReportDialogForAutoSentReport()
    closeErrorReportDialog()
    expect(errorReportFlow.mode).toBe('compose')
  })

  // The amend entry point exists so the auto-sent toast can't reach the compose
  // path; a stale `amend` left behind would send the Help menu there next time.
  it('the compose entry point resets a leftover amend mode', () => {
    openErrorReportDialogForAutoSentReport()
    openErrorReportDialog('from the Help menu')
    expect(errorReportFlow.mode).toBe('compose')
  })
})
