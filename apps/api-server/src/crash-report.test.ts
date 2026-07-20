import { describe, expect, it, vi, type Mock } from 'vitest'
import { app } from './index'

function createMockKv(): KVNamespace {
  return {
    get: vi.fn(() => null),
    put: vi.fn(),
  } as unknown as KVNamespace
}

/** Mock D1Database that tracks prepare/bind/run calls. Returns mocks for assertions. */
function createMockD1(runImpl?: () => Promise<unknown>): {
  db: D1Database
  prepareMock: Mock
  bindMock: Mock
} {
  const run = vi.fn(runImpl ?? (() => Promise.resolve({ success: true })))
  const bindMock = vi.fn(() => ({ run }))
  const prepareMock = vi.fn(() => ({ bind: bindMock }))
  return { db: { prepare: prepareMock } as unknown as D1Database, prepareMock, bindMock }
}

function createMockAnalyticsEngine(): AnalyticsEngineDataset {
  return { writeDataPoint: vi.fn() }
}

function createBindings(overrides: Record<string, unknown> = {}) {
  return {
    LICENSE_CODES: createMockKv(),
    DEVICE_COUNTS: createMockAnalyticsEngine(),
    TELEMETRY_DB: createMockD1().db,
    ED25519_PRIVATE_KEY: 'deadbeef'.repeat(8),
    RESEND_API_KEY: 'test-resend-key',
    PRODUCT_NAME: 'Cmdr',
    SUPPORT_EMAIL: 'test@example.com',
    ADMIN_API_TOKEN: 'test-admin-token-secret',
    ...overrides,
  }
}

const validCrashReport = {
  appVersion: '1.2.3',
  osVersion: '15.3.1',
  arch: 'arm64',
  signal: 'SIGSEGV',
  backtraceFrames: ['std::panic::begin_unwind', 'cmdr::sync_status::get_ubiquitous_bool', 'cmdr_lib::watcher::run'],
}

function postCrashReport(body: unknown, bindings: Record<string, unknown>) {
  return app.request(
    '/crash-report',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    },
    bindings,
  )
}

describe('POST /crash-report', () => {
  it('returns 204 for a valid crash report', async () => {
    const bindings = createBindings()
    const res = await postCrashReport(validCrashReport, bindings)
    expect(res.status).toBe(204)
  })

  it('inserts correct data into D1', async () => {
    const { db, prepareMock, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await postCrashReport(validCrashReport, bindings)

    expect(prepareMock).toHaveBeenCalledOnce()
    const prepareCall = prepareMock.mock.calls[0][0] as string
    expect(prepareCall).toContain('INSERT INTO crash_reports')

    const bindArgs = bindMock.mock.calls[0]
    // bindArgs: [hashedIp, appVersion, osVersion, arch, signal, topFunction, backtraceTruncated, buildMode, shortId]
    expect(bindArgs[0]).toMatch(/^[0-9a-f]{64}$/) // SHA-256 hex
    expect(bindArgs[1]).toBe('1.2.3') // appVersion
    expect(bindArgs[2]).toBe('15.3.1') // osVersion
    expect(bindArgs[3]).toBe('arm64') // arch
    expect(bindArgs[4]).toBe('SIGSEGV') // signal
    expect(bindArgs[5]).toBe('cmdr::sync_status::get_ubiquitous_bool') // topFunction
    expect(bindArgs[7]).toBeNull() // buildMode (not supplied by validCrashReport)
    expect(bindArgs[8]).toBeNull() // shortId (not supplied by validCrashReport)
  })

  it('stores buildMode and shortId when supplied', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const report = { ...validCrashReport, buildMode: 'debug', shortId: 'CRASH-A2345' }

    await postCrashReport(report, bindings)

    const bindArgs = bindMock.mock.calls[0]
    expect(bindArgs[7]).toBe('debug')
    expect(bindArgs[8]).toBe('CRASH-A2345')
  })

  it('accepts explicit nulls for buildMode and shortId (upgrade-window compat)', async () => {
    // Rust clients serialize `Option::None` as `null` because specta's unified mode
    // rejects `skip_serializing_if`. Old crash files read by a new client surface as
    // `None` and would hit this path. Reject would lose the report.
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const report = { ...validCrashReport, buildMode: null, shortId: null }

    const res = await postCrashReport(report, bindings)
    expect(res.status).toBe(204)

    const bindArgs = bindMock.mock.calls[0]
    expect(bindArgs[7]).toBeNull()
    expect(bindArgs[8]).toBeNull()
  })

  it('returns 400 for invalid buildMode', async () => {
    const bindings = createBindings()
    const report = { ...validCrashReport, buildMode: 'staging' }

    const res = await postCrashReport(report, bindings)
    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid buildMode')
  })

  it('returns 400 for malformed shortId', async () => {
    const bindings = createBindings()
    const report = { ...validCrashReport, shortId: 'CRASH-lowercase' }

    const res = await postCrashReport(report, bindings)
    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid shortId')
  })

  it('extracts the first cmdr frame as topFunction', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const report = {
      ...validCrashReport,
      backtraceFrames: ['std::thread::start', 'cmdr_lib::indexer::build_index', 'cmdr::main'],
    }

    await postCrashReport(report, bindings)
    expect(bindMock.mock.calls[0][5]).toBe('cmdr_lib::indexer::build_index')
  })

  it('uses "unknown" when no cmdr frame is found', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const report = {
      ...validCrashReport,
      backtraceFrames: ['std::thread::start', 'tokio::runtime::run'],
    }

    await postCrashReport(report, bindings)
    expect(bindMock.mock.calls[0][5]).toBe('unknown')
  })

  it('uses "unknown" when backtraceFrames is absent', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const reportWithoutFrames = {
      appVersion: validCrashReport.appVersion,
      osVersion: validCrashReport.osVersion,
      arch: validCrashReport.arch,
      signal: validCrashReport.signal,
    }

    await postCrashReport(reportWithoutFrames, bindings)
    expect(bindMock.mock.calls[0][5]).toBe('unknown')
  })

  it('returns 204 even when D1 write fails', async () => {
    const { db } = createMockD1(() => Promise.reject(new Error('D1 unavailable')))
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await postCrashReport(validCrashReport, bindings)
    expect(res.status).toBe(204)
  })

  it('returns 400 for oversized report (> 64 KB)', async () => {
    const bindings = createBindings()
    const oversized = { ...validCrashReport, padding: 'x'.repeat(70_000) }

    const res = await postCrashReport(oversized, bindings)

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Report too large')
  })

  it('returns 400 when required field appVersion is missing', async () => {
    const bindings = createBindings()
    const incomplete = { osVersion: '15.3.1', arch: 'arm64', signal: 'SIGSEGV' }

    const res = await postCrashReport(incomplete, bindings)

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Missing required field: appVersion')
  })

  it('returns 400 when required field osVersion is missing', async () => {
    const bindings = createBindings()
    const incomplete = { appVersion: '1.2.3', arch: 'arm64', signal: 'SIGSEGV' }

    const res = await postCrashReport(incomplete, bindings)

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Missing required field: osVersion')
  })

  it('returns 400 when required field arch is missing', async () => {
    const bindings = createBindings()
    const incomplete = { appVersion: '1.2.3', osVersion: '15.3.1', signal: 'SIGSEGV' }

    const res = await postCrashReport(incomplete, bindings)

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Missing required field: arch')
  })

  it('returns 400 when required field signal is missing', async () => {
    const bindings = createBindings()
    const incomplete = { appVersion: '1.2.3', osVersion: '15.3.1', arch: 'arm64' }

    const res = await postCrashReport(incomplete, bindings)

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Missing required field: signal')
  })

  it('returns 400 when a required field is an empty string', async () => {
    const bindings = createBindings()
    const report = { ...validCrashReport, signal: '' }

    const res = await postCrashReport(report, bindings)

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Missing required field: signal')
  })

  it('returns 400 for malformed JSON', async () => {
    const bindings = createBindings()

    const res = await app.request(
      '/crash-report',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: 'not valid json {{{',
      },
      bindings,
    )

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid JSON')
  })

  it('truncates backtrace to 5,000 bytes', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const longFrames = Array.from(
      { length: 500 },
      (_, i) => `some_very_long_function_name_to_fill_space_${String(i).padStart(4, '0')}`,
    )
    const report = { ...validCrashReport, backtraceFrames: longFrames }

    await postCrashReport(report, bindings)

    const backtrace = bindMock.mock.calls[0][6] as string
    expect(backtrace.length).toBeLessThanOrEqual(5_000)
  })

  it('stores diagId and email when supplied', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const report = {
      ...validCrashReport,
      diagId: 'diag_12345678-1234-1234-1234-1234567890ab',
      email: 'tester@example.com',
    }

    await postCrashReport(report, bindings)

    // bindArgs: [hashedIp, appVersion, osVersion, arch, signal, topFunction, backtraceTruncated, buildMode, shortId, diagId, email]
    const bindArgs = bindMock.mock.calls[0]
    expect(bindArgs[9]).toBe('diag_12345678-1234-1234-1234-1234567890ab')
    expect(bindArgs[10]).toBe('tester@example.com')
  })

  it('accepts explicit nulls for diagId and email (upgrade-window compat)', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const report = { ...validCrashReport, diagId: null, email: null }

    const res = await postCrashReport(report, bindings)
    expect(res.status).toBe(204)

    const bindArgs = bindMock.mock.calls[0]
    expect(bindArgs[9]).toBeNull()
    expect(bindArgs[10]).toBeNull()
  })

  it('round-trips a valid diagId and email through D1', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const report = {
      ...validCrashReport,
      diagId: 'diag_abcdef00-0000-4000-8000-abcdef000000',
      email: 'me@getcmdr.com',
    }

    const res = await postCrashReport(report, bindings)
    expect(res.status).toBe(204)

    const bindArgs = bindMock.mock.calls[0]
    expect(bindArgs[9]).toBe(report.diagId)
    expect(bindArgs[10]).toBe(report.email)
  })

  it('returns 400 for a malformed diagId', async () => {
    const bindings = createBindings()
    const report = { ...validCrashReport, diagId: 'diag_NOT-A-UUID' }

    const res = await postCrashReport(report, bindings)
    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid diagId')
  })

  it('rejects an anal_-prefixed id in the diagId field (analytics id never on reports)', async () => {
    // The unjoinability invariant: a report must never carry the analytics id. An
    // `anal_`-prefixed value in `diagId` fails the `diag_` shape check, so a coding
    // mistake that wired the wrong id can't silently land in D1.
    const bindings = createBindings()
    const report = { ...validCrashReport, diagId: 'anal_12345678-1234-1234-1234-1234567890ab' }

    const res = await postCrashReport(report, bindings)
    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid diagId')
  })

  it('never writes an anal_-prefixed value into any crash column', async () => {
    // Defense in depth: even if a malformed payload slips a valid-shaped value past
    // validation, assert no bound argument carries the analytics prefix.
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const report = {
      ...validCrashReport,
      diagId: 'diag_12345678-1234-1234-1234-1234567890ab',
      email: 'tester@example.com',
    }

    await postCrashReport(report, bindings)

    const bindArgs = bindMock.mock.calls[0] as unknown[]
    for (const arg of bindArgs) {
      if (typeof arg === 'string') {
        expect(arg.startsWith('anal_')).toBe(false)
      }
    }
  })
})

/**
 * Grouping quality: `top_function` is the only column the nightly crash email groups on,
 * so it has to name the code that actually broke. Every panic backtrace starts with the
 * same panic-machinery prelude (the hook, `std::panicking`, `core::panicking`), which
 * collapsed unrelated bugs into one bucket. These use real production backtraces.
 */
describe('top_function derivation', () => {
  async function topFunctionFor(backtraceFrames: string[]): Promise<unknown> {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })
    await postCrashReport({ ...validCrashReport, backtraceFrames }, bindings)
    return bindMock.mock.calls[0][5]
  }

  /** The prelude every panic backtrace carries, verbatim from production reports. */
  const panicPrelude = [
    'std::backtrace::Backtrace::create',
    'cmdr_lib::crash_reporter::install_panic_hook::{{closure}}',
    'std::panicking::rust_panic_with_hook',
    'std::panicking::begin_panic_handler::{{closure}}',
    'std::sys::backtrace::__rust_end_short_backtrace',
    '__rustc::rust_begin_unwind',
    'core::panicking::panic_fmt',
  ]

  it('skips the panic machinery and names the reconciler frame', async () => {
    expect(
      await topFunctionFor([
        ...panicPrelude,
        'core::str::slice_error_fail',
        'cmdr_lib::indexing::reconciler::unknown_path_skips::record',
        'cmdr_lib::indexing::reconciler::reconcile',
      ]),
    ).toBe('cmdr_lib::indexing::reconciler::unknown_path_skips::record')
  })

  it('skips the panic machinery and names the bundle-builder frame', async () => {
    expect(
      await topFunctionFor([
        ...panicPrelude,
        'core::str::slice_error_fail',
        'cmdr_lib::error_reporter::bundle_builder::build_bundle_legacy_window',
        'cmdr_lib::error_reporter::auto_dispatcher::dispatch',
      ]),
    ).toBe('cmdr_lib::error_reporter::bundle_builder::build_bundle_legacy_window')
  })

  it('skips the panic machinery and names the caching frame', async () => {
    expect(
      await topFunctionFor([
        ...panicPrelude,
        'tokio::task::spawn::spawn',
        'cmdr_lib::file_system::listing::caching::notify_directory_changed',
        'cmdr_lib::file_system::git::watcher::refresh_local_listings_under',
      ]),
    ).toBe('cmdr_lib::file_system::listing::caching::notify_directory_changed')
  })

  it('derives distinct buckets for three unrelated real crashes', async () => {
    const derived = [
      await topFunctionFor([...panicPrelude, 'core::str::slice_error_fail', 'cmdr_lib::indexing::reconciler::record']),
      await topFunctionFor([...panicPrelude, 'core::str::slice_error_fail', 'cmdr_lib::error_reporter::bundle::build']),
      await topFunctionFor([...panicPrelude, 'tokio::task::spawn::spawn', 'cmdr_lib::file_system::caching::notify']),
    ]
    expect(new Set(derived).size).toBe(3)
  })

  it('skips unwrap and expect helpers', async () => {
    expect(
      await topFunctionFor([
        ...panicPrelude,
        'core::option::unwrap_failed',
        'core::result::unwrap_failed',
        'core::option::expect_failed',
        'cmdr_lib::settings::load_settings',
      ]),
    ).toBe('cmdr_lib::settings::load_settings')
  })

  it('falls back to unknown when the panic machinery is all there is', async () => {
    expect(await topFunctionFor(panicPrelude)).toBe('unknown')
  })
})

/**
 * The panic message is the single most diagnostic field in a crash report: it turns
 * "something panicked in `caching`" into "there is no reactor running". The client
 * redacts and caps it before it leaves the machine (`crash_reporter::sanitize_panic_message`);
 * the server caps again so a client that skips that step can't blow the column up.
 */
describe('panicMessage', () => {
  /** bindArgs index of `panic_message` in the INSERT. */
  const panicMessageIndex = 11

  it('stores the panic message when supplied', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const report = { ...validCrashReport, panicMessage: 'there is no reactor running' }
    const res = await postCrashReport(report, bindings)

    expect(res.status).toBe(204)
    expect(bindMock.mock.calls[0][panicMessageIndex]).toBe('there is no reactor running')
  })

  it('stores NULL when the field is absent', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await postCrashReport(validCrashReport, bindings)
    expect(bindMock.mock.calls[0][panicMessageIndex]).toBeNull()
  })

  it('accepts an explicit null (Rust serializes Option::None that way)', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await postCrashReport({ ...validCrashReport, panicMessage: null }, bindings)

    expect(res.status).toBe(204)
    expect(bindMock.mock.calls[0][panicMessageIndex]).toBeNull()
  })

  it('rejects a non-string panic message', async () => {
    const bindings = createBindings()
    const res = await postCrashReport({ ...validCrashReport, panicMessage: { evil: true } }, bindings)

    expect(res.status).toBe(400)
    const body = await res.json<{ error: string }>()
    expect(body.error).toBe('Invalid panicMessage')
  })

  it('truncates an over-long panic message instead of rejecting the report', async () => {
    // Losing the whole report over a fat message would be worse than losing its tail.
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const report = { ...validCrashReport, panicMessage: 'y'.repeat(10_000) }
    const res = await postCrashReport(report, bindings)

    expect(res.status).toBe(204)
    const stored = bindMock.mock.calls[0][panicMessageIndex] as string
    expect(stored.length).toBeLessThanOrEqual(2_100)
    expect(stored.endsWith('… (truncated)')).toBe(true)
  })
})
