/**
 * The typed failure of a request to Cmdr's own api server: which log level it earns, and what the
 * person reads. The level is the load-bearing half, because a frontend `log.error` auto-sends an
 * error report: only a failure that means Cmdr and its server disagree may reach it.
 */

import { beforeAll, afterAll, describe, expect, it } from 'vitest'
import type { ServerRequestError } from '$lib/ipc/bindings'
import { _setLocaleForTests } from '$lib/intl/locale'
import {
  ServerRequestFailure,
  describeServerRequestFailure,
  serverRequestFailureOf,
  serverRequestLogLevel,
} from './server-request'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

const refused = (status: number): ServerRequestError => ({ type: 'refused', status, detail: '{"error":"nope"}' })

describe('serverRequestLogLevel', () => {
  it.each<[string, ServerRequestError]>([
    ['no network', { type: 'unreachable', detail: 'dns error' }],
    ['a timeout', { type: 'timedOut', detail: 'deadline has elapsed' }],
    ['a server having a bad moment', refused(503)],
    ['a request timeout status', refused(408)],
    ['a rate limit', refused(429)],
  ])('keeps %s at warn, since the network or the moment is to blame', (_label, failure) => {
    expect(serverRequestLogLevel(failure)).toBe('warn')
  })

  it.each<[string, ServerRequestError | null]>([
    ['a 400', refused(400)],
    ['a 413', refused(413)],
    ['a 2xx this client can’t read', { type: 'badResponse', detail: 'expected value at line 1 column 1' }],
    ['a request Cmdr couldn’t build', { type: 'unexpected', detail: 'HTTP client: builder error' }],
    ['a failure that isn’t typed at all', null],
  ])('raises %s to error, since Cmdr and its server disagree', (_label, failure) => {
    expect(serverRequestLogLevel(failure)).toBe('error')
  })
})

describe('a request the organization’s policy blocked', () => {
  const blocked: ServerRequestError = { type: 'blockedByPolicy' }

  it('logs at info: nothing went wrong, the organization turned it off', () => {
    expect(serverRequestLogLevel(blocked)).toBe('info')
  })

  it('says the organization turned it off, unlike any network or server sentence', () => {
    const sentence = describeServerRequestFailure(blocked)
    expect(sentence).toContain('organization')
    expect(sentence).not.toBe(describeServerRequestFailure({ type: 'unreachable', detail: '' }))
  })

  it('names the policy in the diagnostic', () => {
    expect(new ServerRequestFailure(blocked).message).toContain('blockedByPolicy')
  })
})

describe('describeServerRequestFailure', () => {
  const every: (ServerRequestError | null)[] = [
    { type: 'unreachable', detail: 'error sending request for url (https://api.getcmdr.com/crash-report)' },
    { type: 'timedOut', detail: 'operation timed out' },
    refused(503),
    refused(422),
    { type: 'badResponse', detail: 'error decoding response body' },
    { type: 'unexpected', detail: 'HTTP client: failed' },
    { type: 'blockedByPolicy' },
    null,
  ]

  it.each(every)('words %j from the catalog, without the raw detail or the words "error" and "failed"', (failure) => {
    const sentence = describeServerRequestFailure(failure)
    expect(sentence.length).toBeGreaterThan(0)
    if (failure !== null && 'detail' in failure) expect(sentence).not.toContain(failure.detail)
    expect(sentence.toLowerCase()).not.toMatch(/\berror\b|\bfailed\b/)
  })

  it('tells someone offline to check their connection, and someone refused that it isn’t their network', () => {
    const offline = describeServerRequestFailure({ type: 'unreachable', detail: '' })
    const turnedDown = describeServerRequestFailure(refused(422))
    expect(offline).not.toBe(turnedDown)
    expect(offline).toContain('connection')
  })
})

describe('serverRequestFailureOf', () => {
  it('gives the typed failure back from a thrown carrier, and null from anything else', () => {
    const failure: ServerRequestError = { type: 'timedOut', detail: 'deadline' }
    expect(serverRequestFailureOf(new ServerRequestFailure(failure))).toEqual(failure)
    expect(serverRequestFailureOf(new Error('bridge broke'))).toBeNull()
  })

  it('carries the detail in the diagnostic, for the log', () => {
    expect(new ServerRequestFailure(refused(400)).message).toContain('400')
  })
})
