/**
 * A request to Cmdr's own api server that didn't land: the typed failure across the throw, the log
 * level it earns, and the sentence a person reads.
 *
 * Classification is the backend's (`src-tauri/src/server_request.rs`), shared by every sender that
 * talks to the api server: the crash report, the error report and its amend, the update check.
 *
 * The level is the load-bearing half. A frontend `log.error` auto-sends an error report for people
 * who opted in, so only a failure that means Cmdr and its server disagree may reach it: a 4xx, a
 * body this client can't read, a request Cmdr couldn't build, or a failure that isn't typed at all
 * (the IPC bridge itself broke). No network, a timeout, a 5xx, and a rate limit are the person's
 * network or the server's bad moment, so they stay at warn.
 *
 * Copy comes from `errors.serverRequest.*` through `getMessage()`, a raw lookup like the rest of
 * `errors.*`. ❌ The failure's `detail` never reaches a person: it rides the diagnostic, for the log.
 */
import type { ServerRequestError } from '$lib/ipc/bindings'
import { getMessage } from '$lib/intl/messages.svelte'
import { TypedFailure, failureOf } from '$lib/ipc/typed-failure'

/** The log line for a failure: its type, the status for a refusal, and the backend's detail. */
export function serverRequestDiagnostic(failure: ServerRequestError): string {
  if (failure.type === 'blockedByPolicy') return 'server request blockedByPolicy'
  const status = failure.type === 'refused' ? ` ${String(failure.status)}` : ''
  return `server request ${failure.type}${status}: ${failure.detail}`
}

/** An `Error` that still carries the backend's typed request failure. */
export class ServerRequestFailure extends TypedFailure<ServerRequestError> {
  constructor(failure: ServerRequestError) {
    super(failure, serverRequestDiagnostic(failure))
    this.name = 'ServerRequestFailure'
  }
}

/** Throws a wire `ServerRequestError` as an `Error`, keeping the typed value. */
export function throwServerRequestError(failure: ServerRequestError): never {
  throw new ServerRequestFailure(failure)
}

/** The typed failure behind a caught value, or `null` when it isn't one. */
export function serverRequestFailureOf(error: unknown): ServerRequestError | null {
  return failureOf(ServerRequestFailure, error)
}

/** 5xx, plus the two 4xx statuses that mean "not now" rather than "not like this". */
function isServerTrouble(status: number): boolean {
  return status >= 500 || status === 408 || status === 429
}

/**
 * Warn when the network or the moment is to blame; error when Cmdr and its server disagree; info when the
 * organization's managed policy turned the pipeline off (nothing went wrong, and nothing left the Mac).
 */
export function serverRequestLogLevel(failure: ServerRequestError | null): 'info' | 'warn' | 'error' {
  if (failure === null) return 'error'
  switch (failure.type) {
    case 'blockedByPolicy':
      return 'info'
    case 'unreachable':
    case 'timedOut':
      return 'warn'
    case 'refused':
      return isServerTrouble(failure.status) ? 'warn' : 'error'
    case 'badResponse':
    case 'unexpected':
      return 'error'
  }
}

/** One or two plain sentences saying what happened and what to do. `null` is an untyped failure. */
export function describeServerRequestFailure(failure: ServerRequestError | null): string {
  if (failure === null) return getMessage('errors.serverRequest.unexpected')
  switch (failure.type) {
    case 'unreachable':
      return getMessage('errors.serverRequest.unreachable')
    case 'timedOut':
      return getMessage('errors.serverRequest.timedOut')
    case 'refused':
      return getMessage(
        isServerTrouble(failure.status) ? 'errors.serverRequest.serverBusy' : 'errors.serverRequest.refused',
      )
    case 'badResponse':
      return getMessage('errors.serverRequest.refused')
    case 'unexpected':
      return getMessage('errors.serverRequest.unexpected')
    case 'blockedByPolicy':
      return getMessage('errors.serverRequest.blockedByPolicy')
  }
}
