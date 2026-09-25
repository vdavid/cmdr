/**
 * The typed reason an error-report send or amend didn't land: it survives the throw, and every
 * variant reads as a catalog sentence, never as the backend's detail.
 */

import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import type { ErrorReportSendError } from '$lib/ipc/bindings'
import { _setLocaleForTests } from '$lib/intl/locale'
import { formatInteger } from '$lib/intl/number-format'
import { tString } from '$lib/intl/messages.svelte'
import { describeServerRequestFailure } from '$lib/error-messages/server-request'
import {
  ErrorReportSendFailure,
  errorReportSendFailureOf,
  errorReportSendReason,
  throwErrorReportSendError,
} from './error-report-send-error'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

const every: ErrorReportSendError[] = [
  { type: 'server', failure: { type: 'refused', status: 413, detail: '{"error":"Bundle too large"}' } },
  { type: 'bundleUnavailable', detail: 'read log dir: Permission denied (os error 13)' },
  { type: 'noteTooLong', maxChars: 100_000 },
  { type: 'notAmendable' },
]

describe('ErrorReportSendFailure', () => {
  it.each(every)('carries %j across a throw', (failure) => {
    let caught: unknown
    try {
      throwErrorReportSendError(failure)
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ErrorReportSendFailure)
    expect(errorReportSendFailureOf(caught)).toEqual(failure)
  })

  it('reads a failure that isn’t one of its own as null', () => {
    expect(errorReportSendFailureOf(new Error('bridge broke'))).toBeNull()
  })
})

describe('errorReportSendReason', () => {
  it('words a request that didn’t land the way every api-server surface does', () => {
    const failure = { type: 'unreachable', detail: 'dns error' } as const
    expect(errorReportSendReason({ type: 'server', failure })).toBe(describeServerRequestFailure(failure))
  })

  it('says the logs couldn’t be gathered, without the file system’s own words', () => {
    expect(errorReportSendReason({ type: 'bundleUnavailable', detail: 'Permission denied' })).toBe(
      tString('errorReporter.dialog.bundleUnavailable'),
    )
  })

  it('names the cap for a note over it', () => {
    expect(errorReportSendReason({ type: 'noteTooLong', maxChars: 100_000 })).toBe(
      tString('errorReporter.dialog.noteTooLong', { maxText: formatInteger(100_000), max: 100_000 }),
    )
  })

  it('points an amend with nothing to add to at a new report', () => {
    expect(errorReportSendReason({ type: 'notAmendable' })).toBe(tString('errorReporter.amend.unavailable'))
  })

  it('words an untyped failure like a request Cmdr couldn’t build', () => {
    expect(errorReportSendReason(null)).toBe(describeServerRequestFailure(null))
  })
})
