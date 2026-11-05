/**
 * CI health against a REAL SQLite and a fake GitHub: an ops event on each green→red and red→green
 * change, on silence, and one line when GitHub can't be read, never a thrown job.
 */
import { afterEach, beforeEach, describe, expect, it, vi, type MockInstance } from 'vitest'
import { createSqliteD1, type SqliteD1 } from './sqlite-d1-test-helper'
import { handleCiHealth } from './ci-health'
import type { WorkflowJob, WorkflowRun } from './ci-github'
import type { Bindings } from './types'

const NOW = new Date('2026-11-05T12:00:00Z')
const HOUR = 3_600_000

function hoursAgo(hours: number): string {
  return new Date(NOW.getTime() - hours * HOUR).toISOString().replace(/\.\d{3}Z$/, 'Z')
}

let nextRunId = 1000

function run(conclusion: string | null, overrides: Partial<WorkflowRun> = {}): WorkflowRun {
  const id = nextRunId++
  return {
    id,
    html_url: `https://github.com/vdavid/cmdr/actions/runs/${String(id)}`,
    head_sha: `${String(id)}abcdef0123456789`,
    event: 'push',
    status: conclusion === null ? 'in_progress' : 'completed',
    conclusion,
    created_at: hoursAgo(2),
    head_commit: { message: `fix(thing): commit ${String(id)}\n\nBody text` },
    ...overrides,
  }
}

interface FakeCi {
  /** Keyed `<workflow>` or `<workflow>:<event>`; a missing key answers 404. */
  runs: Partial<Record<string, WorkflowRun[]>>
  jobs?: Partial<Record<number, WorkflowJob[]>>
  /** `main`'s head commit; defaults to the newest `ci.yml:push` run's commit, committed 2 h ago. */
  head?: { sha: string; date: string }
  /** Answer every request with this status. */
  down?: number
}

let fetchMock: MockInstance<typeof fetch>

function fakeGitHub(ci: FakeCi) {
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input: RequestInfo | URL) => {
    const url = new URL(input as string)
    if (ci.down) return Promise.resolve(new Response('{}', { status: ci.down }))
    const workflowRuns = /^\/repos\/vdavid\/cmdr\/actions\/workflows\/([^/]+)\/runs$/.exec(url.pathname)
    if (workflowRuns) {
      const workflow = workflowRuns[1]
      const event = url.searchParams.get('event')
      const runs = ci.runs[event ? `${workflow}:${event}` : workflow]
      if (!runs) return Promise.resolve(new Response('{"message":"Not Found"}', { status: 404 }))
      return Promise.resolve(Response.json({ total_count: runs.length, workflow_runs: runs }))
    }
    const jobs = /^\/repos\/vdavid\/cmdr\/actions\/runs\/(\d+)\/jobs$/.exec(url.pathname)
    if (jobs) return Promise.resolve(Response.json({ jobs: ci.jobs?.[Number(jobs[1])] ?? [] }))
    if (url.pathname === '/repos/vdavid/cmdr/commits/main') {
      const newestPush = ci.runs['ci.yml:push']?.[0]
      const head = ci.head ?? { sha: newestPush?.head_sha ?? 'deadbeef', date: hoursAgo(2) }
      return Promise.resolve(Response.json({ sha: head.sha, commit: { committer: { date: head.date } } }))
    }
    return Promise.resolve(new Response('unexpected', { status: 418 }))
  })
}

/** A healthy repo: every watched workflow green, the nightly ran 10 h ago, `main`'s head has its run. */
function healthy(overrides: Partial<FakeCi['runs']> = {}): FakeCi['runs'] {
  return {
    'ci.yml:push': [run('success')],
    'ci.yml:schedule': [run('success', { event: 'schedule', created_at: hoursAgo(10) })],
    'ci-upkeep.yml': [run('success')],
    'slow-checks.yml': [run('success', { event: 'schedule' })],
    'advisories.yml': [run('success', { event: 'schedule' })],
    ...overrides,
  }
}

let db: SqliteD1

function env(overrides: Partial<Bindings> = {}): Bindings {
  return { TELEMETRY_DB: db.d1, CI_HEALTH_REPO: 'vdavid/cmdr', ...overrides } as Bindings
}

function opsEvents(): { kind: string; message: string }[] {
  return db.sqlite.prepare(`SELECT kind, message FROM ops_events ORDER BY id`).all() as {
    kind: string
    message: string
  }[]
}

beforeEach(() => {
  db = createSqliteD1()
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('handleCiHealth', () => {
  it('stays quiet on a first look at a green repo, and remembers it', async () => {
    fakeGitHub({ runs: healthy() })
    await handleCiHealth(env(), NOW)
    expect(opsEvents()).toEqual([])
    const states = db.sqlite.prepare(`SELECT signal, state FROM ci_health ORDER BY signal`).all()
    expect(states).toEqual([
      { signal: 'advisories', state: 'green' },
      { signal: 'github', state: 'ok' },
      { signal: 'main', state: 'green' },
      { signal: 'main-ran', state: 'ok' },
      { signal: 'nightly', state: 'green' },
      { signal: 'nightly-ran', state: 'ok' },
      { signal: 'slow-checks', state: 'green' },
      { signal: 'upkeep', state: 'green' },
    ])
  })

  it('writes one event when the nightly goes red, naming the commit, the failed jobs, and the run', async () => {
    fakeGitHub({ runs: healthy() })
    await handleCiHealth(env(), NOW)

    const red = run('failure', { event: 'schedule', created_at: hoursAgo(9) })
    fakeGitHub({
      runs: healthy({ 'ci.yml:schedule': [red] }),
      jobs: {
        [red.id]: [
          {
            name: 'Desktop (Rust) / Kani proofs',
            conclusion: 'failure',
            labels: ['ubuntu-latest'],
            completed_at: null,
          },
          { name: 'Website / Checks', conclusion: 'success', labels: ['ubuntu-latest'], completed_at: null },
          { name: 'CI OK', conclusion: 'failure', labels: ['ubuntu-latest'], completed_at: null },
        ],
      },
    })
    await handleCiHealth(env(), NOW)

    const events = opsEvents()
    expect(events).toHaveLength(1)
    expect(events[0].kind).toBe('ci-health')
    expect(events[0].message).toBe(
      `The nightly full run went red on main at 2026-11-05 03:00 UTC (commit ${red.head_sha.slice(0, 7)}, "fix(thing): commit ${String(red.id)}"). Failed: Desktop (Rust) / Kani proofs, CI OK. Run: ${red.html_url}`,
    )
  })

  it('stays quiet while a red stays red, then says when it is back to green', async () => {
    fakeGitHub({ runs: healthy({ 'ci.yml:push': [run('failure')] }) })
    await handleCiHealth(env(), NOW)
    expect(opsEvents()).toHaveLength(1)
    expect(opsEvents()[0].message).toMatch(/^CI on main \(push\) went red on main/)

    fakeGitHub({ runs: healthy({ 'ci.yml:push': [run('timed_out')] }) })
    await handleCiHealth(env(), NOW)
    expect(opsEvents()).toHaveLength(1)

    const green = run('success')
    fakeGitHub({ runs: healthy({ 'ci.yml:push': [green] }) })
    await handleCiHealth(env(), NOW)
    expect(opsEvents()).toHaveLength(2)
    expect(opsEvents()[1].message).toBe(
      `CI on main (push) is back to green (commit ${green.head_sha.slice(0, 7)}, "fix(thing): commit ${String(green.id)}"). Run: ${green.html_url}`,
    )
  })

  it('reads past cancelled, skipped, and running runs to the newest one that decided something', async () => {
    const failed = run('failure')
    fakeGitHub({
      runs: healthy({ 'ci.yml:push': [run(null), run('cancelled'), run('skipped'), failed, run('success')] }),
      head: { sha: 'whatever', date: hoursAgo(0.1) },
    })
    await handleCiHealth(env(), NOW)
    expect(opsEvents()).toHaveLength(1)
    expect(opsEvents()[0].message).toContain(failed.html_url)
  })

  it('treats a missing workflow file as absent, not as GitHub being down', async () => {
    const runs = healthy()
    delete runs['ci-upkeep.yml']
    fakeGitHub({ runs })
    await handleCiHealth(env(), NOW)
    expect(opsEvents()).toEqual([])
    const upkeep = db.sqlite.prepare(`SELECT state FROM ci_health WHERE signal = 'upkeep'`).get()
    expect(upkeep).toBeUndefined()
  })

  it("turns a GitHub outage into one line, never a thrown job, and doesn't repeat it", async () => {
    fakeGitHub({ runs: {}, down: 502 })
    await expect(handleCiHealth(env(), NOW)).resolves.toBeUndefined()
    expect(opsEvents()).toEqual([
      {
        kind: 'ci-health',
        message:
          "Couldn't check CI: GitHub answered 502 for ci.yml runs. CI alerts wait until GitHub answers again; the next tick retries.",
      },
    ])

    await handleCiHealth(env(), NOW)
    expect(opsEvents()).toHaveLength(1)
  })

  it('never changes a state it could not read during an outage', async () => {
    fakeGitHub({ runs: healthy({ 'ci.yml:push': [run('failure')] }) })
    await handleCiHealth(env(), NOW)
    fakeGitHub({ runs: {}, down: 403 })
    await handleCiHealth(env(), NOW)
    const main = db.sqlite.prepare(`SELECT state FROM ci_health WHERE signal = 'main'`).get()
    expect(main).toEqual({ state: 'red' })
  })

  it('alerts once when the nightly has not run in 26 hours, and again when it is back', async () => {
    fakeGitHub({
      runs: healthy({ 'ci.yml:schedule': [run('success', { event: 'schedule', created_at: hoursAgo(30) })] }),
    })
    await handleCiHealth(env(), NOW)
    expect(opsEvents().map((event) => event.message)).toEqual([
      "The nightly full run hasn't started in 26 hours (the last one started 2026-11-04 06:00 UTC). Check that the CI workflow is enabled (gh workflow enable CI) and that its schedule is still in ci.yml.",
    ])

    await handleCiHealth(env(), NOW)
    expect(opsEvents()).toHaveLength(1)

    fakeGitHub({ runs: healthy() })
    await handleCiHealth(env(), NOW)
    expect(opsEvents()[1].message).toBe(
      'The nightly full run is running again (the last one started 2026-11-05 02:00 UTC).',
    )
  })

  it('says nothing about silence before the nightly has ever run (a schedule added today)', async () => {
    fakeGitHub({ runs: healthy({ 'ci.yml:schedule': [] }) })
    await handleCiHealth(env(), NOW)
    expect(opsEvents()).toEqual([])
  })

  it("alerts when main's head has had no CI run for over an hour, and not before", async () => {
    fakeGitHub({ runs: healthy(), head: { sha: 'abcdef1234567', date: hoursAgo(0.5) } })
    await handleCiHealth(env(), NOW)
    expect(opsEvents()).toEqual([])

    fakeGitHub({ runs: healthy(), head: { sha: 'abcdef1234567', date: hoursAgo(1.5) } })
    await handleCiHealth(env(), NOW)
    expect(opsEvents().map((event) => event.message)).toEqual([
      "main's head commit abcdef1 (committed 2026-11-05 10:30 UTC) has no CI run after an hour, so nothing checked or deployed it. Check GitHub Actions' status page and that the CI workflow is enabled.",
    ])

    fakeGitHub({ runs: healthy() })
    await handleCiHealth(env(), NOW)
    expect(opsEvents()[1].message).toMatch(/^main's head commit \w{7} has a CI run again\.$/)
  })

  it('is off without CI_HEALTH_REPO', async () => {
    fakeGitHub({ runs: healthy() })
    await handleCiHealth(env({ CI_HEALTH_REPO: undefined }), NOW)
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('sends the project token when there is one', async () => {
    fakeGitHub({ runs: healthy() })
    await handleCiHealth(env({ GITHUB_PROJECT_TOKEN: 'ghp_project' }), NOW)
    const init = fetchMock.mock.calls[0][1] as RequestInit
    expect((init.headers as Record<string, string>).authorization).toBe('Bearer ghp_project')
  })
})
