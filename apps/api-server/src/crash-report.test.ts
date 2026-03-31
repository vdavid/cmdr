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
  return { writeDataPoint: vi.fn() } as unknown as AnalyticsEngineDataset
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
    // bindArgs: [hashedIp, appVersion, osVersion, arch, signal, topFunction, backtraceTruncated]
    expect(bindArgs[0]).toMatch(/^[0-9a-f]{64}$/) // SHA-256 hex
    expect(bindArgs[1]).toBe('1.2.3') // appVersion
    expect(bindArgs[2]).toBe('15.3.1') // osVersion
    expect(bindArgs[3]).toBe('arm64') // arch
    expect(bindArgs[4]).toBe('SIGSEGV') // signal
    expect(bindArgs[5]).toBe('cmdr::sync_status::get_ubiquitous_bool') // topFunction
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
})
