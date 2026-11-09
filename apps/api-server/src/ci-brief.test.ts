/**
 * The morning brief's CI line against a REAL SQLite (the states `ci-health.ts` saved) and a fake
 * GitHub: lands, their median CI time, how many macOS lanes ran on the M1, and the cache size.
 */
import { afterEach, beforeEach, describe, expect, it, vi, type MockInstance } from 'vitest'
import { createSqliteD1, type SqliteD1 } from './sqlite-d1-test-helper'
import { formatCiLine, readCiBriefLine } from './ci-brief'
import type { WorkflowJob, WorkflowRun } from './ci-github'
import type { Bindings } from './types'

const NOW = new Date('2026-11-06T05:30:00Z')
const MINUTE = 60_000

function minutesAgo(minutes: number): string {
  return new Date(NOW.getTime() - minutes * MINUTE).toISOString()
}

interface FakeLand {
  sha: string
  mergedMinutesAgo: number
  /** The land run's runs, newest first; each with its jobs. */
  runs: { run: Partial<WorkflowRun>; jobs: WorkflowJob[] }[]
}

function job(
  name: string,
  conclusion: string | null,
  labels: string[],
  completedAt: string | null = null,
): WorkflowJob {
  return { name, conclusion, labels, completed_at: completedAt }
}

/** A land run that started `startedMinutesAgo` and whose `CI OK` went green `minutes` later. */
function landRun(
  id: number,
  startedMinutesAgo: number,
  minutes: number,
  macos: 'm1' | 'hosted' | 'skipped',
  gate: 'CI OK' | 'CI light' = 'CI OK',
): { run: Partial<WorkflowRun>; jobs: WorkflowJob[] } {
  const macLabels = macos === 'm1' ? ['self-hosted', 'cmdr-m1'] : ['macos-26']
  return {
    run: { id, status: 'completed', conclusion: 'success', run_started_at: minutesAgo(startedMinutesAgo) },
    jobs: [
      job('Detect changes', 'success', ['ubuntu-latest']),
      job(
        'Desktop (Rust, macOS) / Clippy, rustdoc, tests, module cycles',
        macos === 'skipped' ? 'skipped' : 'success',
        macLabels,
      ),
      job('Desktop (Rust, macOS) / Lane marker', 'success', ['ubuntu-latest']),
      job(gate, 'success', ['ubuntu-latest'], minutesAgo(startedMinutesAgo - minutes)),
    ],
  }
}

let fetchMock: MockInstance<typeof fetch>

function fakeGitHub(lands: FakeLand[], options: { cacheBytes?: number; down?: boolean } = {}) {
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input: RequestInfo | URL) => {
    const url = new URL(input as string)
    if (options.down) return Promise.resolve(new Response('{}', { status: 503 }))
    if (url.pathname === '/repos/getcmdr/cmdr/pulls') {
      return Promise.resolve(
        Response.json([
          ...lands.map((land) => ({ merged_at: minutesAgo(land.mergedMinutesAgo), head: { sha: land.sha } })),
          { merged_at: null, head: { sha: 'closed-unmerged' } },
        ]),
      )
    }
    if (url.pathname === '/repos/getcmdr/cmdr/actions/workflows/ci.yml/runs') {
      const land = lands.find((candidate) => candidate.sha === url.searchParams.get('head_sha'))
      return Promise.resolve(Response.json({ workflow_runs: land?.runs.map((entry) => entry.run) ?? [] }))
    }
    const jobs = /^\/repos\/getcmdr\/cmdr\/actions\/runs\/(\d+)\/jobs$/.exec(url.pathname)
    if (jobs) {
      const entry = lands.flatMap((land) => land.runs).find((candidate) => candidate.run.id === Number(jobs[1]))
      return Promise.resolve(Response.json({ jobs: entry?.jobs ?? [] }))
    }
    if (url.pathname === '/repos/getcmdr/cmdr/actions/cache/usage') {
      return Promise.resolve(Response.json({ active_caches_size_in_bytes: options.cacheBytes ?? 0 }))
    }
    return Promise.resolve(new Response('unexpected', { status: 418 }))
  })
}

let db: SqliteD1

function env(overrides: Partial<Bindings> = {}): Bindings {
  return { TELEMETRY_DB: db.d1, CI_HEALTH_REPO: 'getcmdr/cmdr', ...overrides } as Bindings
}

function saveState(signal: string, state: string) {
  db.sqlite
    .prepare(`INSERT INTO ci_health (signal, state, changed_at, checked_at) VALUES (?, ?, 'x', 'x')`)
    .run(signal, state)
}

beforeEach(() => {
  db = createSqliteD1()
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('formatCiLine', () => {
  it('reads like the brief David asked for', () => {
    expect(
      formatCiLine({
        main: 'green',
        nightly: 'green',
        lands: { count: 9, medianMinutes: 21, macosRan: 9, macosOnM1: 8, capped: false },
        cacheBytes: 6.1 * 1024 ** 3,
      }),
    ).toBe('CI: main green · nightly green · 9 lands, median 21 min · macOS 8 of 9 on the M1 · cache 6.1 GB')
  })

  it('says what it could not read, and leaves out a macOS count no land ran', () => {
    expect(formatCiLine({ main: null, nightly: 'red', lands: null, cacheBytes: null })).toBe(
      "CI: main not checked yet · nightly red · couldn't read lands from GitHub · cache size unknown",
    )
    expect(
      formatCiLine({
        main: 'red',
        nightly: 'green',
        lands: { count: 1, medianMinutes: 30, macosRan: 0, macosOnM1: 0, capped: false },
        cacheBytes: 0,
      }),
    ).toBe('CI: main red · nightly green · 1 land, median 30 min · cache 0.0 GB')
    expect(
      formatCiLine({
        main: 'green',
        nightly: 'green',
        lands: { count: 0, medianMinutes: null, macosRan: 0, macosOnM1: 0, capped: false },
        cacheBytes: 0,
      }),
    ).toBe('CI: main green · nightly green · no lands · cache 0.0 GB')
  })
})

describe('readCiBriefLine', () => {
  it('counts the last day of lands, their median CI time, the M1 share, and the cache', async () => {
    saveState('main', 'green')
    saveState('nightly', 'red')
    fakeGitHub(
      [
        { sha: 'a', mergedMinutesAgo: 60, runs: [landRun(1, 100, 20, 'm1')] },
        { sha: 'b', mergedMinutesAgo: 300, runs: [landRun(2, 400, 30, 'hosted')] },
        { sha: 'c', mergedMinutesAgo: 600, runs: [landRun(3, 700, 22, 'skipped')] },
        // A light run on the same head, newer than its land run: passed over for the land run.
        { sha: 'd', mergedMinutesAgo: 900, runs: [landRun(5, 990, 5, 'm1', 'CI light'), landRun(4, 1000, 25, 'm1')] },
        // Merged over a day ago: not counted.
        { sha: 'e', mergedMinutesAgo: 1500, runs: [landRun(6, 1600, 40, 'm1')] },
      ],
      { cacheBytes: 6_549_825_126 },
    )

    expect(await readCiBriefLine(env(), NOW)).toBe(
      'CI: main green · nightly red · 4 lands, median 24 min · macOS 2 of 3 on the M1 · cache 6.1 GB',
    )
  })

  it('leaves out a merged PR whose runs never reported a green CI OK', async () => {
    fakeGitHub([{ sha: 'a', mergedMinutesAgo: 60, runs: [landRun(1, 100, 20, 'm1', 'CI light')] }])
    expect(await readCiBriefLine(env(), NOW)).toBe(
      'CI: main not checked yet · nightly not checked yet · no lands · cache 0.0 GB',
    )
  })

  it("degrades to what it couldn't read, never throwing, when GitHub is down", async () => {
    saveState('main', 'green')
    saveState('nightly', 'green')
    fakeGitHub([], { down: true })
    expect(await readCiBriefLine(env(), NOW)).toBe(
      "CI: main green · nightly green · couldn't read lands from GitHub · cache size unknown",
    )
  })

  it('is off without CI_HEALTH_REPO', async () => {
    fakeGitHub([])
    expect(await readCiBriefLine(env({ CI_HEALTH_REPO: undefined }), NOW)).toBeNull()
    expect(fetchMock).not.toHaveBeenCalled()
  })
})
