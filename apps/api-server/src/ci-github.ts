/**
 * Read-only GitHub Actions calls for CI health (`ci-health.ts`) and the morning brief's CI line
 * (`ci-brief.ts`), against the PUBLIC repo `CI_HEALTH_REPO` names.
 *
 * Every read here works without a token on a public repo. With `GITHUB_PROJECT_TOKEN` set it rides
 * along anyway: a classic PAT reads public repos whatever its scopes, and unauthenticated calls from
 * a Worker share Cloudflare's egress IPs, whose 60-an-hour budget other tenants spend too. The token
 * is only ever SENT here, never widened (`project-board.ts`).
 */
import { githubFetch } from './github-fetch'
import type { Bindings } from './types'

const GITHUB_API = 'https://api.github.com'

/** One `workflow_runs` entry, the fields we read. */
export interface WorkflowRun {
  id: number
  html_url: string
  head_sha: string
  event: string
  status: string
  /** `success`, `failure`, `timed_out`, `startup_failure`, `cancelled`, `skipped`, … or null while running. */
  conclusion: string | null
  created_at: string
  run_started_at?: string
  head_commit?: { message?: string } | null
}

/** One job of a run, the fields we read. */
export interface WorkflowJob {
  name: string
  conclusion: string | null
  /** The `runs-on` labels, which say where the job ran (`cmdr-m1`, `macos-26`, `ubuntu-latest`). */
  labels: string[]
  completed_at: string | null
}

export interface MergedPull {
  merged_at: string | null
  head: { sha: string }
}

/** GitHub answered something other than a usable 2xx (or nothing at all). `status` is null for a network failure. */
export class CiGitHubError extends Error {
  constructor(
    readonly status: number | null,
    readonly what: string,
  ) {
    super(status === null ? `GitHub didn't answer ${what}` : `GitHub answered ${String(status)} for ${what}`)
    this.name = 'CiGitHubError'
  }
}

/** The repo CI health watches, or null when `CI_HEALTH_REPO` is unset (the feature is off). */
export function ciRepo(env: Bindings): string | null {
  return env.CI_HEALTH_REPO?.trim() || null
}

/**
 * GET one API path (`/repos/...`). Null on a 404 (a workflow file that doesn't exist yet), a
 * {@link CiGitHubError} on anything else that isn't a 2xx. `what` names the read in that error.
 */
export async function ciGet<T>(env: Bindings, path: string, what: string): Promise<T | null> {
  const headers: Record<string, string> = {
    accept: 'application/vnd.github+json',
    'user-agent': 'cmdr-api-server',
    'x-github-api-version': '2022-11-28',
  }
  if (env.GITHUB_PROJECT_TOKEN) headers.authorization = `Bearer ${env.GITHUB_PROJECT_TOKEN}`
  let response: Response
  try {
    response = await githubFetch(`${GITHUB_API}${path}`, { headers })
  } catch {
    throw new CiGitHubError(null, what)
  }
  if (response.status === 404) return null
  if (!response.ok) throw new CiGitHubError(response.status, what)
  return response.json<T>()
}

/** The newest runs of one workflow file (newest first), or null when the file doesn't exist. */
export async function listWorkflowRuns(
  env: Bindings,
  repo: string,
  workflow: string,
  query: Record<string, string>,
): Promise<WorkflowRun[] | null> {
  const params = new URLSearchParams(query).toString()
  const page = await ciGet<{ workflow_runs: WorkflowRun[] }>(
    env,
    `/repos/${repo}/actions/workflows/${workflow}/runs?${params}`,
    `${workflow} runs`,
  )
  return page ? page.workflow_runs : null
}

/** The latest attempt's jobs of one run. */
export async function listRunJobs(env: Bindings, repo: string, runId: number): Promise<WorkflowJob[]> {
  const page = await ciGet<{ jobs: WorkflowJob[] }>(
    env,
    `/repos/${repo}/actions/runs/${String(runId)}/jobs?filter=latest&per_page=100`,
    `run ${String(runId)}'s jobs`,
  )
  return page?.jobs ?? []
}
