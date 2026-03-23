import { describe, expect, it, vi } from 'vitest'
import app from './index'

function createMockKv(): KVNamespace {
    return {
        get: vi.fn(() => null),
        put: vi.fn(),
    } as unknown as KVNamespace
}

function createMockAnalyticsEngine(): AnalyticsEngineDataset {
    return { writeDataPoint: vi.fn() } as unknown as AnalyticsEngineDataset
}

function createBindings(overrides: Record<string, unknown> = {}) {
    return {
        LICENSE_CODES: createMockKv(),
        DOWNLOADS: createMockAnalyticsEngine(),
        DEVICE_COUNTS: createMockAnalyticsEngine(),
        UPDATE_CHECKS: createMockAnalyticsEngine(),
        CRASH_REPORTS: createMockAnalyticsEngine(),
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

/** Extract the first writeDataPoint call arg from a mock analytics engine. */
function getWrittenDataPoint(engine: AnalyticsEngineDataset) {
    const [dataPoint] = vi.mocked(engine).writeDataPoint.mock.calls[0]
    if (!dataPoint) throw new Error('writeDataPoint was never called')
    return dataPoint
}

describe('POST /crash-report', () => {
    it('returns 204 for a valid crash report', async () => {
        const bindings = createBindings()
        const res = await app.request(
            '/crash-report',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(validCrashReport),
            },
            bindings,
        )

        expect(res.status).toBe(204)
    })

    it('writes correct data to Analytics Engine', async () => {
        const crashReports = createMockAnalyticsEngine()
        const bindings = createBindings({ CRASH_REPORTS: crashReports })

        await app.request(
            '/crash-report',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(validCrashReport),
            },
            bindings,
        )

        const call = getWrittenDataPoint(crashReports)
        expect(call.indexes).toHaveLength(1)
        expect(call.indexes?.[0]).toMatch(/^[0-9a-f]{64}$/) // SHA-256 hex
        expect(call.blobs?.[0]).toBe('1.2.3') // appVersion
        expect(call.blobs?.[1]).toBe('15.3.1') // osVersion
        expect(call.blobs?.[2]).toBe('arm64') // arch
        expect(call.blobs?.[3]).toBe('SIGSEGV') // signal
        expect(call.blobs?.[4]).toBe('cmdr::sync_status::get_ubiquitous_bool') // topFunction
        expect(call.doubles).toEqual([1])
    })

    it('extracts the first cmdr frame as topFunction', async () => {
        const crashReports = createMockAnalyticsEngine()
        const bindings = createBindings({ CRASH_REPORTS: crashReports })

        const report = {
            ...validCrashReport,
            backtraceFrames: ['std::thread::start', 'cmdr_lib::indexer::build_index', 'cmdr::main'],
        }

        await app.request(
            '/crash-report',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(report),
            },
            bindings,
        )

        const call = getWrittenDataPoint(crashReports)
        expect(call.blobs?.[4]).toBe('cmdr_lib::indexer::build_index')
    })

    it('uses "unknown" when no cmdr frame is found', async () => {
        const crashReports = createMockAnalyticsEngine()
        const bindings = createBindings({ CRASH_REPORTS: crashReports })

        const report = {
            ...validCrashReport,
            backtraceFrames: ['std::thread::start', 'tokio::runtime::run'],
        }

        await app.request(
            '/crash-report',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(report),
            },
            bindings,
        )

        const call = getWrittenDataPoint(crashReports)
        expect(call.blobs?.[4]).toBe('unknown')
    })

    it('uses "unknown" when backtraceFrames is absent', async () => {
        const crashReports = createMockAnalyticsEngine()
        const bindings = createBindings({ CRASH_REPORTS: crashReports })

        const reportWithoutFrames = {
            appVersion: validCrashReport.appVersion,
            osVersion: validCrashReport.osVersion,
            arch: validCrashReport.arch,
            signal: validCrashReport.signal,
        }

        await app.request(
            '/crash-report',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(reportWithoutFrames),
            },
            bindings,
        )

        const call = getWrittenDataPoint(crashReports)
        expect(call.blobs?.[4]).toBe('unknown')
    })

    it('returns 400 for oversized report (> 64 KB)', async () => {
        const bindings = createBindings()
        const oversized = { ...validCrashReport, padding: 'x'.repeat(70_000) }

        const res = await app.request(
            '/crash-report',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(oversized),
            },
            bindings,
        )

        expect(res.status).toBe(400)
        const body = await res.json<{ error: string }>()
        expect(body.error).toBe('Report too large')
    })

    it('returns 400 when required field appVersion is missing', async () => {
        const bindings = createBindings()
        const incomplete = { osVersion: '15.3.1', arch: 'arm64', signal: 'SIGSEGV' }

        const res = await app.request(
            '/crash-report',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(incomplete),
            },
            bindings,
        )

        expect(res.status).toBe(400)
        const body = await res.json<{ error: string }>()
        expect(body.error).toBe('Missing required field: appVersion')
    })

    it('returns 400 when required field osVersion is missing', async () => {
        const bindings = createBindings()
        const incomplete = { appVersion: '1.2.3', arch: 'arm64', signal: 'SIGSEGV' }

        const res = await app.request(
            '/crash-report',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(incomplete),
            },
            bindings,
        )

        expect(res.status).toBe(400)
        const body = await res.json<{ error: string }>()
        expect(body.error).toBe('Missing required field: osVersion')
    })

    it('returns 400 when required field arch is missing', async () => {
        const bindings = createBindings()
        const incomplete = { appVersion: '1.2.3', osVersion: '15.3.1', signal: 'SIGSEGV' }

        const res = await app.request(
            '/crash-report',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(incomplete),
            },
            bindings,
        )

        expect(res.status).toBe(400)
        const body = await res.json<{ error: string }>()
        expect(body.error).toBe('Missing required field: arch')
    })

    it('returns 400 when required field signal is missing', async () => {
        const bindings = createBindings()
        const incomplete = { appVersion: '1.2.3', osVersion: '15.3.1', arch: 'arm64' }

        const res = await app.request(
            '/crash-report',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(incomplete),
            },
            bindings,
        )

        expect(res.status).toBe(400)
        const body = await res.json<{ error: string }>()
        expect(body.error).toBe('Missing required field: signal')
    })

    it('returns 400 when a required field is an empty string', async () => {
        const bindings = createBindings()
        const report = { ...validCrashReport, signal: '' }

        const res = await app.request(
            '/crash-report',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(report),
            },
            bindings,
        )

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
        const crashReports = createMockAnalyticsEngine()
        const bindings = createBindings({ CRASH_REPORTS: crashReports })

        const longFrames = Array.from(
            { length: 500 },
            (_, i) => `some_very_long_function_name_to_fill_space_${String(i).padStart(4, '0')}`,
        )
        const report = { ...validCrashReport, backtraceFrames: longFrames }

        await app.request(
            '/crash-report',
            {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(report),
            },
            bindings,
        )

        const call = getWrittenDataPoint(crashReports)
        const backtrace = call.blobs?.[5] as string
        expect(backtrace.length).toBeLessThanOrEqual(5_000)
    })
})
