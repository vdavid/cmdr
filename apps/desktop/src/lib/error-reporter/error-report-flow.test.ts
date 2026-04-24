/**
 * Tests for the error-report flow store.
 *
 * The store has two operations: open (with optional initial note) and close.
 * Both menu items and toast buttons funnel through `openErrorReportDialog`,
 * so this is the right place to assert the open/close contract.
 */

import { describe, it, expect, beforeEach } from 'vitest'
import { errorReportFlow, openErrorReportDialog, closeErrorReportDialog } from './error-report-flow.svelte'

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
})
