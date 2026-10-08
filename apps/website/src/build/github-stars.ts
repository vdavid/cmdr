// The repo's GitHub star count, read once at build time so the visitor's browser never calls GitHub.
// Lives in `src/build/`: import it from component frontmatter only, never from a client `<script>`
// (the `website-csp-connect-src` check skips this directory on that promise).
// The site rebuilds on every deploy and release, which keeps the number fresh enough for a progress
// line. Best-effort: any failure (rate limit, network, timeout) yields `null`, and callers drop the
// number rather than show a wrong one.

export const GITHUB_REPO_URL = 'https://github.com/vdavid/cmdr'

/** Stars Homebrew wants before it accepts a developer's own cask into the main catalog. */
export const HOMEBREW_STAR_GOAL = 225

const API_URL = 'https://api.github.com/repos/vdavid/cmdr'
const TIMEOUT_MS = 5_000

let cached: Promise<number | null> | null = null

async function fetchStarCount(): Promise<number | null> {
  try {
    const res = await fetch(API_URL, {
      headers: { Accept: 'application/vnd.github+json' },
      signal: AbortSignal.timeout(TIMEOUT_MS),
    })
    if (!res.ok) return null
    const data = (await res.json()) as { stargazers_count?: unknown }
    return typeof data.stargazers_count === 'number' ? data.stargazers_count : null
  } catch {
    return null
  }
}

/** Memoized per process, so a build (or a dev server's page reloads) asks GitHub once. */
export function getGithubStarCount(): Promise<number | null> {
  cached ??= fetchStarCount()
  return cached
}
