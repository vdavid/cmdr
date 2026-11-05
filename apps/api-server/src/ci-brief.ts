/**
 * The morning brief's one CI line, real numbers for the last 24 hours:
 * `CI: main green · nightly green · 9 lands, median 21 min · macOS 8 of 9 on the M1 · cache 6.1 GB`.
 *
 * - **main / nightly**: the verdicts `ci-health.ts` last saved (D1, no GitHub call), at most a
 *   tick old.
 * - **A land**: a PR merged into `main` in the last 24 h whose `ci.yml` `pull_request` run reported
 *   a green `CI OK` job on its head commit. Only a land run names its gate `CI OK` (a light run's is
 *   `CI light`), and `main`'s ruleset requires that job BY NAME, so the name is the contract.
 * - **Median**: from the land run's `run_started_at` to its `CI OK` job's `completed_at`, in
 *   minutes. A rerun counts from its latest attempt's start.
 * - **macOS on the M1**: of the lands whose macOS lane ran (a non-skipped job on a `cmdr-m1` or
 *   `macos-*` runner label), how many ran it on a `cmdr-m1` label.
 * - **Cache**: `actions/cache/usage`'s `active_caches_size_in_bytes`, in GiB.
 *
 * Bounded: one PR listing, at most {@link MAX_LANDS} lands with one run listing and up to
 * {@link RUNS_TRIED_PER_LAND} job listings each, and one cache read. Never throws on GitHub: what it
 * couldn't read, the line says so.
 */
import { ciGet, ciRepo, listRunJobs, listWorkflowRuns, type MergedPull, type WorkflowJob } from './ci-github'
import type { Bindings } from './types'

/** The most lands one brief looks up; past it the count reads `30+`. */
const MAX_LANDS = 30
/** Runs per land head tried for a green `CI OK`, newest first (a light run can sit in front of the land run). */
const RUNS_TRIED_PER_LAND = 3
/** The gate job a land run names, and `main`'s ruleset requires by this name. */
const LAND_GATE_JOB = 'CI OK'
const DAY_MS = 86_400_000

type Verdict = 'green' | 'red'

export interface LandStats {
  count: number
  medianMinutes: number | null
  /** Lands whose macOS lane ran. */
  macosRan: number
  /** Of those, the ones that ran it on the M1. */
  macosOnM1: number
  /** More than {@link MAX_LANDS} PRs merged; only that many were looked up. */
  capped: boolean
}

export interface CiBriefNumbers {
  main: Verdict | null
  nightly: Verdict | null
  /** Null when GitHub couldn't be read. */
  lands: LandStats | null
  cacheBytes: number | null
}

function verdictWord(verdict: Verdict | null): string {
  return verdict ?? 'not checked yet'
}

export function formatCiLine(numbers: CiBriefNumbers): string {
  const parts = [`main ${verdictWord(numbers.main)}`, `nightly ${verdictWord(numbers.nightly)}`]
  const { lands } = numbers
  if (!lands) parts.push("couldn't read lands from GitHub")
  else if (lands.count === 0) parts.push('no lands')
  else {
    const count = `${String(lands.count)}${lands.capped ? '+' : ''} ${lands.count === 1 ? 'land' : 'lands'}`
    parts.push(lands.medianMinutes === null ? count : `${count}, median ${String(lands.medianMinutes)} min`)
    if (lands.macosRan > 0) parts.push(`macOS ${String(lands.macosOnM1)} of ${String(lands.macosRan)} on the M1`)
  }
  parts.push(
    numbers.cacheBytes === null ? 'cache size unknown' : `cache ${(numbers.cacheBytes / 1024 ** 3).toFixed(1)} GB`,
  )
  return `CI: ${parts.join(' · ')}`
}

function median(values: number[]): number | null {
  if (values.length === 0) return null
  const sorted = [...values].sort((a, b) => a - b)
  const middle = Math.floor(sorted.length / 2)
  const value = sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2
  return Math.round(value)
}

function ran(job: WorkflowJob): boolean {
  return job.conclusion !== null && job.conclusion !== 'skipped'
}

function onM1(job: WorkflowJob): boolean {
  return job.labels.includes('cmdr-m1')
}

function onMac(job: WorkflowJob): boolean {
  return onM1(job) || job.labels.some((label) => label.startsWith('macos'))
}

interface Land {
  minutes: number | null
  macosRan: boolean
  macosOnM1: boolean
}

/** The land behind one merged head: its newest run with a green `CI OK`, or null when it has none. */
async function findLand(env: Bindings, repo: string, sha: string): Promise<Land | null> {
  const runs =
    (await listWorkflowRuns(env, repo, 'ci.yml', { head_sha: sha, event: 'pull_request', per_page: '10' })) ?? []
  const completed = runs.filter((run) => run.status === 'completed').slice(0, RUNS_TRIED_PER_LAND)
  for (const run of completed) {
    const jobs = await listRunJobs(env, repo, run.id)
    const gate = jobs.find((job) => job.name === LAND_GATE_JOB && job.conclusion === 'success')
    if (!gate) continue
    const started = run.run_started_at ? Date.parse(run.run_started_at) : NaN
    const ended = gate.completed_at ? Date.parse(gate.completed_at) : NaN
    const macJobs = jobs.filter((job) => ran(job) && onMac(job))
    return {
      minutes: Number.isFinite(started) && Number.isFinite(ended) ? (ended - started) / 60_000 : null,
      macosRan: macJobs.length > 0,
      macosOnM1: macJobs.some(onM1),
    }
  }
  return null
}

async function readLands(env: Bindings, repo: string, now: Date): Promise<LandStats> {
  const pulls =
    (await ciGet<MergedPull[]>(
      env,
      // 50 covers MAX_LANDS with room; a PR object is ~25 KB, so a bigger page is CPU for nothing.
      `/repos/${repo}/pulls?state=closed&base=main&sort=updated&direction=desc&per_page=50`,
      'closed PRs',
    )) ?? []
  const merged = pulls.filter((pull) => pull.merged_at && now.getTime() - Date.parse(pull.merged_at) <= DAY_MS)
  const lands: Land[] = []
  for (const pull of merged.slice(0, MAX_LANDS)) {
    const land = await findLand(env, repo, pull.head.sha)
    if (land) lands.push(land)
  }
  return {
    count: lands.length,
    medianMinutes: median(lands.map((land) => land.minutes).filter((minutes): minutes is number => minutes !== null)),
    macosRan: lands.filter((land) => land.macosRan).length,
    macosOnM1: lands.filter((land) => land.macosOnM1).length,
    capped: merged.length > MAX_LANDS,
  }
}

async function readVerdicts(db: D1Database): Promise<{ main: Verdict | null; nightly: Verdict | null }> {
  const rows = await db
    .prepare(`SELECT signal, state FROM ci_health WHERE signal IN ('main', 'nightly')`)
    .all<{ signal: string; state: string }>()
  const verdict = (signal: string): Verdict | null => {
    const state = rows.results.find((row) => row.signal === signal)?.state
    return state === 'green' || state === 'red' ? state : null
  }
  return { main: verdict('main'), nightly: verdict('nightly') }
}

/** Swallow a failed read into null, logged, so one part of the line can't take down the brief. */
async function orNull<T>(what: string, read: () => Promise<T>): Promise<T | null> {
  try {
    return await read()
  } catch (e) {
    console.error(`Morning brief: reading ${what} for the CI line failed`, e)
    return null
  }
}

/** The brief's CI line, or null without `CI_HEALTH_REPO`. Never throws. */
export async function readCiBriefLine(env: Bindings, now: Date = new Date()): Promise<string | null> {
  const repo = ciRepo(env)
  if (!repo) return null
  const verdicts = (await orNull('the saved CI states', () => readVerdicts(env.TELEMETRY_DB))) ?? {
    main: null,
    nightly: null,
  }
  const lands = await orNull('lands', () => readLands(env, repo, now))
  const cache = await orNull('the cache size', () =>
    ciGet<{ active_caches_size_in_bytes: number }>(env, `/repos/${repo}/actions/cache/usage`, 'the cache usage'),
  )
  return formatCiLine({ ...verdicts, lands, cacheBytes: cache?.active_caches_size_in_bytes ?? null })
}
