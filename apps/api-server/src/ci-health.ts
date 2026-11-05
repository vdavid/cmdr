/**
 * CI health, every three-hour tick: ask GitHub how `main`'s workflows last ended and write a
 * `ci-health` ops event only when that CHANGES, so a red `main` reaches David through the digest he
 * reads (GitHub's own failure emails are off).
 *
 * - **Verdicts** (`green` / `red`): the newest run that decided something, per watched workflow
 *   ({@link WATCHED}). `success` is green; `failure`, `timed_out`, and `startup_failure` are red;
 *   anything else (cancelled, skipped, still running) is passed over. Read from `conclusion` and
 *   `status` alone, never from a title or a log line.
 * - **Silence** (`ok` / `silent`): the nightly hasn't started in {@link NIGHTLY_SILENT_HOURS} (judged
 *   only once GitHub lists one schedule run), or `main`'s head commit is older than
 *   {@link HEAD_RUN_GRACE_HOURS} by its committer date with no `ci.yml` push run. The committer date
 *   stands in for "landed at" because GitHub doesn't expose push times; a push run appears within
 *   seconds, so an old-dated commit that just landed can't be mistaken for one without a run.
 * - **Reachability** (`ok` / `unreachable`): any read GitHub refused. One line on the way down,
 *   none on the way back (the next real change says it works). A state it couldn't read is left as
 *   it was, so an outage never fakes a recovery.
 *
 * Throws only when D1 does, which `runCronJob` turns into the usual cron alarm. A GitHub failure
 * never throws.
 */
import { CiGitHubError, ciGet, ciRepo, listRunJobs, listWorkflowRuns, type WorkflowRun } from './ci-github'
import { recordOpsEvent } from './ops-events'
import type { Bindings } from './types'

/** A watched workflow: whose newest decided run on `main` is one green-or-red signal. */
interface Watched {
  signal: string
  /** How the alert names it, at the start of a sentence. */
  label: string
  workflow: string
  /** Only runs from this trigger, when one workflow carries two signals. */
  event?: string
}

/** `ci-upkeep.yml` may not exist yet; a missing file answers 404 and is skipped. */
export const WATCHED: readonly Watched[] = [
  { signal: 'main', label: 'CI on main (push)', workflow: 'ci.yml', event: 'push' },
  { signal: 'nightly', label: 'The nightly full run', workflow: 'ci.yml', event: 'schedule' },
  { signal: 'upkeep', label: 'CI upkeep', workflow: 'ci-upkeep.yml' },
  { signal: 'slow-checks', label: 'Slow checks', workflow: 'slow-checks.yml' },
  { signal: 'advisories', label: 'The advisory scan', workflow: 'advisories.yml' },
]

const VERDICTS: Readonly<Partial<Record<string, 'green' | 'red'>>> = {
  success: 'green',
  failure: 'red',
  timed_out: 'red',
  startup_failure: 'red',
}

/** The nightly fires daily at 02:17 UTC; two more hours absorbs GitHub's schedule delays. */
export const NIGHTLY_SILENT_HOURS = 26
/** A push run is created within seconds; an hour without one means GitHub dropped it or the workflow is off. */
export const HEAD_RUN_GRACE_HOURS = 1
/** Enough runs per read to see past a burst of cancelled ones. */
const RUNS_PER_READ = 20
/** The most failed job names an alert lists. */
const FAILED_JOBS_SHOWN = 5

const HOUR_MS = 3_600_000

/** `2026-11-05 03:00 UTC`. */
export function utcMinute(iso: string): string {
  return `${new Date(iso).toISOString().slice(0, 16).replace('T', ' ')} UTC`
}

function shortSha(sha: string): string {
  return sha.slice(0, 7)
}

/** `commit abc1234, "fix(x): title"`, the title cut to its first line. */
function commitPhrase(run: WorkflowRun): string {
  const title = run.head_commit?.message?.split('\n')[0]?.trim()
  if (!title) return `commit ${shortSha(run.head_sha)}`
  const cut = title.length > 80 ? `${title.slice(0, 79)}…` : title
  return `commit ${shortSha(run.head_sha)}, "${cut}"`
}

/** The newest run that decided something, with its verdict. */
export function decidingRun(runs: WorkflowRun[]): { run: WorkflowRun; verdict: 'green' | 'red' } | null {
  for (const run of runs) {
    if (run.status !== 'completed' || run.conclusion === null) continue
    const verdict = VERDICTS[run.conclusion]
    if (verdict) return { run, verdict }
  }
  return null
}

/** The last state seen for `signal`, then `state` saved as the new one. Returns the previous (null the first time). */
async function swapState(db: D1Database, signal: string, state: string, now: Date): Promise<string | null> {
  const previous = await db
    .prepare(`SELECT state FROM ci_health WHERE signal = ?`)
    .bind(signal)
    .first<{ state: string }>()
  const at = now.toISOString()
  await db
    .prepare(
      `INSERT INTO ci_health (signal, state, changed_at, checked_at) VALUES (?, ?, ?, ?)
       ON CONFLICT (signal) DO UPDATE SET
         changed_at = CASE WHEN ci_health.state = excluded.state THEN ci_health.changed_at ELSE excluded.changed_at END,
         state = excluded.state,
         checked_at = excluded.checked_at`,
    )
    .bind(signal, state, at, at)
    .run()
  return previous?.state ?? null
}

/** The names of a red run's failed jobs, or an empty list when they can't be read (the alert goes out regardless). */
async function failedJobs(env: Bindings, repo: string, runId: number): Promise<string[]> {
  try {
    const jobs = await listRunJobs(env, repo, runId)
    return jobs.filter((job) => job.conclusion !== null && VERDICTS[job.conclusion] === 'red').map((job) => job.name)
  } catch {
    return []
  }
}

async function redMessage(env: Bindings, repo: string, watched: Watched, run: WorkflowRun): Promise<string> {
  const failed = await failedJobs(env, repo, run.id)
  const shown = failed.slice(0, FAILED_JOBS_SHOWN)
  const more = failed.length > shown.length ? `, and ${String(failed.length - shown.length)} more` : ''
  const jobs = shown.length ? ` Failed: ${shown.join(', ')}${more}.` : ''
  return `${watched.label} went red on main at ${utcMinute(run.created_at)} (${commitPhrase(run)}).${jobs} Run: ${run.html_url}`
}

function greenMessage(watched: Watched, run: WorkflowRun): string {
  return `${watched.label} is back to green (${commitPhrase(run)}). Run: ${run.html_url}`
}

function nightlySilentMessage(latest: WorkflowRun): string {
  return `The nightly full run hasn't started in ${String(NIGHTLY_SILENT_HOURS)} hours (the last one started ${utcMinute(latest.created_at)}). Check that the CI workflow is enabled (gh workflow enable CI) and that its schedule is still in ci.yml.`
}

function describeFailure(e: unknown): string {
  if (e instanceof CiGitHubError) return e.message
  return `reading GitHub threw (${String(e)})`
}

/** Save `state` for `signal` and record `message(previous)` when it returns one. */
async function observe(
  env: Bindings,
  now: Date,
  signal: string,
  state: string,
  message: (previous: string | null) => Promise<string | null> | string | null,
): Promise<void> {
  const previous = await swapState(env.TELEMETRY_DB, signal, state, now)
  if (previous === state) return
  const text = await message(previous)
  if (text) await recordOpsEvent(env.TELEMETRY_DB, 'ci-health', text)
}

/** Every watched workflow's newest runs by signal, plus the first read GitHub refused (null when none did). */
async function readAllRuns(
  env: Bindings,
  repo: string,
): Promise<{ runs: Map<string, WorkflowRun[]>; failure: string | null }> {
  let failure: string | null = null
  const runs = new Map<string, WorkflowRun[]>()
  for (const watched of WATCHED) {
    try {
      const query: Record<string, string> = { branch: 'main', per_page: String(RUNS_PER_READ) }
      if (watched.event) query.event = watched.event
      const list = await listWorkflowRuns(env, repo, watched.workflow, query)
      if (list) runs.set(watched.signal, list)
    } catch (e) {
      failure ??= describeFailure(e)
    }
  }
  return { runs, failure }
}

/**
 * The nightly's silence. No schedule run on record at all means the schedule is new (or was never
 * there): nothing to measure silence from yet, so the state waits for the first run.
 */
async function observeNightlyRan(env: Bindings, now: Date, latest: WorkflowRun | undefined): Promise<void> {
  if (!latest) return
  const silent = now.getTime() - Date.parse(latest.created_at) > NIGHTLY_SILENT_HOURS * HOUR_MS
  await observe(env, now, 'nightly-ran', silent ? 'silent' : 'ok', (previous) => {
    if (silent) return nightlySilentMessage(latest)
    return previous === 'silent'
      ? `The nightly full run is running again (the last one started ${utcMinute(latest.created_at)}).`
      : null
  })
}

/** Whether `main`'s head commit has a push run. Throws a {@link CiGitHubError} when the head can't be read. */
async function observeMainRan(env: Bindings, repo: string, now: Date, pushRuns: WorkflowRun[]): Promise<void> {
  const head = await ciGet<{ sha: string; commit: { committer: { date: string } } }>(
    env,
    `/repos/${repo}/commits/main`,
    "main's head commit",
  )
  if (!head) return
  const hasRun = pushRuns.some((run) => run.head_sha === head.sha)
  const committed = head.commit.committer.date
  // Too young to judge: leave the state alone.
  if (!hasRun && now.getTime() - Date.parse(committed) <= HEAD_RUN_GRACE_HOURS * HOUR_MS) return
  await observe(env, now, 'main-ran', hasRun ? 'ok' : 'silent', (previous) => {
    if (!hasRun) {
      return `main's head commit ${shortSha(head.sha)} (committed ${utcMinute(committed)}) has no CI run after an hour, so nothing checked or deployed it. Check GitHub Actions' status page and that the CI workflow is enabled.`
    }
    return previous === 'silent' ? `main's head commit ${shortSha(head.sha)} has a CI run again.` : null
  })
}

/** The three-hour job. A no-op without `CI_HEALTH_REPO`. */
export async function handleCiHealth(env: Bindings, now: Date = new Date()): Promise<void> {
  const repo = ciRepo(env)
  if (!repo) return

  const { runs, failure: runsFailure } = await readAllRuns(env, repo)
  let failure = runsFailure

  for (const watched of WATCHED) {
    const decided = decidingRun(runs.get(watched.signal) ?? [])
    if (!decided) continue
    await observe(env, now, watched.signal, decided.verdict, async (previous) => {
      if (decided.verdict === 'red') return redMessage(env, repo, watched, decided.run)
      return previous === 'red' ? greenMessage(watched, decided.run) : null
    })
  }

  await observeNightlyRan(env, now, runs.get('nightly')?.[0])

  const pushRuns = runs.get('main')
  if (pushRuns) {
    try {
      await observeMainRan(env, repo, now, pushRuns)
    } catch (e) {
      // Only GitHub's refusal is "couldn't check"; a D1 failure goes on to the cron alarm.
      if (!(e instanceof CiGitHubError)) throw e
      failure ??= describeFailure(e)
    }
  }

  await observe(env, now, 'github', failure ? 'unreachable' : 'ok', () =>
    failure ? `Couldn't check CI: ${failure}. CI alerts wait until GitHub answers again; the next tick retries.` : null,
  )
}
