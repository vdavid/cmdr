/**
 * Carrying a typed `ErrorReportSendError` from `sendErrorReport` / `amendErrorReport` to the dialog
 * that words it.
 *
 * The request itself fails the way every call to Cmdr's api server does (`server`, worded by
 * `$lib/error-messages/server-request`); the other three variants are the dialog's own: the bundle
 * couldn't be built, the note is over the cap, or there's no auto-sent report to add a note to.
 * ❌ No backend string reaches the toast: `detail` rides the diagnostic, for the log.
 */
import type { ErrorReportSendError } from '$lib/ipc/bindings'
import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'
import { TypedFailure, failureOf } from '$lib/ipc/typed-failure'
import { describeServerRequestFailure, serverRequestDiagnostic } from '$lib/error-messages/server-request'

function diagnosticOf(failure: ErrorReportSendError): string {
  switch (failure.type) {
    case 'server':
      return `error report: ${serverRequestDiagnostic(failure.failure)}`
    case 'bundleUnavailable':
      return `error report: couldn't build the bundle: ${failure.detail}`
    case 'noteTooLong':
      return `error report: the note is over ${String(failure.maxChars)} chars`
    case 'notAmendable':
      return 'error report: no auto-sent report can take a note'
  }
}

/** An `Error` that still carries the typed reason a send or an amend didn't land. */
export class ErrorReportSendFailure extends TypedFailure<ErrorReportSendError> {
  constructor(failure: ErrorReportSendError) {
    super(failure, diagnosticOf(failure))
    this.name = 'ErrorReportSendFailure'
  }
}

/** Throws a wire `ErrorReportSendError` as an `Error`, keeping the typed value. */
export function throwErrorReportSendError(failure: ErrorReportSendError): never {
  throw new ErrorReportSendFailure(failure)
}

/** The typed reason behind a caught value, or `null` when it isn't one. */
export function errorReportSendFailureOf(error: unknown): ErrorReportSendError | null {
  return failureOf(ErrorReportSendFailure, error)
}

/**
 * Why a send or an amend didn't land, as the sentence that follows the toast's lead.
 * `null` is an untyped failure (the IPC bridge itself broke), worded like a request Cmdr couldn't
 * build.
 */
export function errorReportSendReason(failure: ErrorReportSendError | null): string {
  if (failure === null) return describeServerRequestFailure(null)
  switch (failure.type) {
    case 'server':
      return describeServerRequestFailure(failure.failure)
    case 'bundleUnavailable':
      return tString('errorReporter.dialog.bundleUnavailable')
    case 'noteTooLong':
      return tString('errorReporter.dialog.noteTooLong', {
        maxText: formatInteger(failure.maxChars),
        max: failure.maxChars,
      })
    case 'notAmendable':
      return tString('errorReporter.amend.unavailable')
  }
}
