/**
 * Measured numbers for `/trust/development`, the page that answers a security questionnaire's
 * "Do you have a secure development lifecycle (SDL), and do you keep the software up to date?".
 * `src/pages/trust/development.astro` is the template and owns the prose.
 *
 * ❗ Every number here was measured on `measuredOn`, and each block says how. Re-measure them all
 * together (same window, same day) and bump `measuredOn`, or the page mixes dates. Commands run
 * from the repo root; the D1 queries need `CLOUDFLARE_API_TOKEN` and are read-only
 * (`pnpm exec wrangler d1 execute cmdr-telemetry --remote --command "..."` from `apps/api-server`).
 * Write straight quotes; `smart-quotes.ts` curls them in the built HTML.
 */

/** The day the numbers below were measured, and the three-month window the rates cover. */
export const measuredOn = '2026-09-23'
export const window = { from: '2026-06-23', to: '2026-09-23', days: 92, weeks: 14 }

/**
 * Releases published in the window, from GitHub's release dates (the moment users can get them):
 * `gh release list --limit 100 --json tagName,publishedAt`, keep `publishedAt >= window.from`,
 * sort, and take the gaps between neighbors (21 gaps for 22 releases). Tag dates are commit dates
 * (older tags are lightweight), so don't use `git tag` for this.
 */
export const releaseCadence = {
  releases: 22,
  first: '0.30.0',
  last: '0.46.1',
  medianGapDays: 2.8,
  longestGapDays: 9.6,
}

/**
 * Minutes from tag push to a published release: `gh run list --workflow release.yml --limit 8
 * --json displayTitle,createdAt,updatedAt,conclusion`, the four most recent successful tag runs
 * (0.44.0: 49, 0.45.0: 43, 0.45.1: 48, 0.46.1: 35). Runs with the `attest` job keep going a few
 * minutes after the release is out, so when refreshing this, take the `publish` job's `completedAt` (`gh run view <id> --json jobs`) rather than `updatedAt`.
 */
export const releaseBuildMinutes = { min: 35, max: 50 }

/**
 * How often locked dependency versions change:
 * `git log --since=<from> --until=<to+1> --format='%ad|%an' --date=short -- Cargo.lock pnpm-lock.yaml`.
 * Count the commits, the distinct days, the distinct ISO weeks, and the `renovate[bot]` rows. This
 * counts every lockfile change (updates, plus dependencies added or removed with a feature), which
 * is why the page says "changed", never "updated".
 */
export const lockfileChanges = { commits: 139, days: 63, weeksWithAChange: 14, byRenovate: 10 }

/**
 * Update adoption, from the `heartbeat` table (usage stats, so installs with stats off are
 * invisible). For a release published at T: take release-build heartbeats in (T+A h, T+B h], keep
 * each install's LAST heartbeat in that window, and count the installs whose last one ran the new
 * version. Summed over four releases with at least three days before the next one:
 * 0.37.0 (2026-08-03 06:03 UTC), 0.39.0 (2026-08-19 21:03), 0.41.0 (2026-08-26 22:12), and 0.46.1
 * (2026-09-19 06:10). Per release, day one / day three: 16 of 27 / 24 of 33, 13 of 36 / 20 of 29,
 * 14 of 29 / 18 of 28, 48 of 83 / 53 of 72.
 */
export const updateAdoption = {
  firstDay: { onNewVersion: 91, activeInstalls: 175, percent: 52 },
  thirdDay: { onNewVersion: 115, activeInstalls: 162, percent: 71 },
}

export interface AdvisoryFix {
  id: string
  crate: string
  /** The `date` field of the advisory in RustSec's advisory database. */
  published: string
  /** The commit on `main` that moved off the affected version. */
  fixCommit: string
  fixedOn: string
  release: string
  /** The release's GitHub publish date. */
  releasedOn: string
  /** Days from `published` to `releasedOn`. */
  days: number
  note?: string
}

/**
 * Every RustSec advisory Cmdr fixed in the window (by release date), from the `### Security`
 * sections of `CHANGELOG.md`, newest first. Dates: `~/.cargo/advisory-db/crates/<crate>/<id>.md` (`date =`),
 * `git log -S<id>` or the fix commit's date, and the release's `publishedAt`.
 */
export const advisoryFixes: AdvisoryFix[] = [
  {
    id: 'RUSTSEC-2026-0285',
    crate: 'rustls',
    published: '2026-09-14',
    fixCommit: '1c7057a09',
    fixedOn: '2026-09-16',
    release: '0.46.0',
    releasedOn: '2026-09-17',
    days: 3,
  },
  {
    id: 'RUSTSEC-2026-0258',
    crate: 'h2',
    published: '2026-08-17',
    fixCommit: 'e21872235',
    fixedOn: '2026-08-19',
    release: '0.39.0',
    releasedOn: '2026-08-19',
    days: 2,
  },
  {
    id: 'RUSTSEC-2026-0221',
    crate: 'event-listener',
    published: '2026-07-13',
    fixCommit: '63a0858f3',
    fixedOn: '2026-08-03',
    release: '0.38.0',
    releasedOn: '2026-08-11',
    days: 29,
    note: 'an "unsound code" warning, not a known exploit',
  },
  {
    id: 'RUSTSEC-2026-0185',
    crate: 'quinn-proto',
    published: '2026-06-22',
    fixCommit: '584aa27fb',
    fixedOn: '2026-06-25',
    release: '0.30.0',
    releasedOn: '2026-06-28',
    days: 6,
    note: "found by the full scan; today's Mac build doesn't include this crate",
  },
  {
    id: 'RUSTSEC-2026-0186',
    crate: 'memmap2',
    published: '2026-06-20',
    fixCommit: '584aa27fb',
    fixedOn: '2026-06-25',
    release: '0.30.0',
    releasedOn: '2026-06-28',
    days: 8,
    note: 'an "unsound code" warning',
  },
]

/**
 * Test and code counts, from `rg -c` over the source (a count of test attributes and test
 * calls, so parameterized cases count once):
 * - Rust tests: `rg -c --type rust '^\s*#\[(tokio::)?test' apps crates`, summed. Write operations:
 *   the same under `apps/desktop/src-tauri/src/file_system/write_operations`.
 * - Frontend unit tests: `rg -c -g '*.test.ts' '^\s*it\(' apps/desktop/src`, summed, and the file count.
 * - End-to-end tests: `rg -c -g '*.spec.ts' '^\s*test(\.skip|\.only)?\(' apps/desktop/test/e2e-playwright`.
 * - `unsafe` blocks: `rg -c --type rust 'unsafe \{' apps crates`. The vendored copy is
 *   `crates/fsevent-stream`: the only crate with `unsafe` code that lacks `[lints] workspace = true`,
 *   so the `undocumented_unsafe_blocks` lint doesn't reach it.
 * - Checks: the count in `scripts/check/CLAUDE.md`'s first line.
 */
export const codeCounts = {
  rustTests: 9514,
  writeOperationTests: 1471,
  frontendTests: 10527,
  frontendTestFiles: 879,
  e2eTests: 371,
  unsafeBlocks: 469,
  unsafeBlocksInVendoredCode: 39,
  checks: 135,
}

/**
 * CI on `main` in the window: `gh run list --workflow ci.yml --branch main --event push
 * --created '>=<from>' --limit 1000 --json conclusion,displayTitle`. Release commits are the
 * `chore(release): v…` rows; `gh run view <id> --json jobs` names the failing job. Only David sees
 * these (in a `DevTodo`); the public page states the process, not the red rate.
 */
export const ciOnMain = {
  runs: 416,
  failed: 192,
  releaseCommits: 21,
  releaseCommitsRed: 9,
  redJobs: 'three Website, three Desktop (Rust), two Linux E2E, one Desktop (Svelte)',
}
