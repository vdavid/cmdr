/**
 * E2E tests for the virtual `.git` portal.
 *
 * These tests build a small synthetic git repository under the fixture root,
 * point a pane at it, and verify the portal reveals branches/tags/commits
 * etc. as virtual folders. The synthesized-at-test-time approach keeps the
 * fixtures lean (no checked-in tarball, no pack-file fragility).
 *
 * Scenarios:
 * 1. Navigate into `.git` and see the virtual portal entries.
 * 2. Navigate into `.git/branches/` and see the branch ref.
 * 3. Navigate into a branch and see the working-tree files at HEAD.
 * 4. Copy a file from the history pane to the working-tree pane and verify
 *    byte-equal AND executable bit preserved.
 * 5. Toggle `fileExplorer.git.showVirtualGitPortal` off and verify navigating
 *    into `.git` shows the raw on-disk contents instead.
 */

import fs from 'fs'
import path from 'path'
import { execSync } from 'child_process'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { test, expect } from './fixtures.js'
import { ensureAppReady, getFixtureRoot, fileExistsInPane, pollUntil } from './helpers.js'

/** Matches the `PageLike` alias used inside `helpers.ts`. */
type PageLike = TauriPage | BrowserPageAdapter

const REPO_REL = 'git-portal-repo'

function repoPath(): string {
  return path.join(getFixtureRoot(), REPO_REL)
}

/**
 * Builds a deterministic git repo inside the fixture root. Idempotent —
 * tears down any prior copy first so individual test runs start clean.
 *
 * Layout:
 * - `README.md`            (regular file, stable content)
 * - `scripts/run.sh`       (executable file, mode 0755)
 * - `branches/main` HEAD ➜ commits these two files
 */
function createGitRepoFixture(): void {
  const repo = repoPath()
  if (fs.existsSync(repo)) fs.rmSync(repo, { recursive: true, force: true })
  fs.mkdirSync(repo, { recursive: true })

  const readme = path.join(repo, 'README.md')
  fs.writeFileSync(readme, '# Git portal fixture\n\nSynthesized at test time.\n')

  const scripts = path.join(repo, 'scripts')
  fs.mkdirSync(scripts, { recursive: true })
  const runSh = path.join(scripts, 'run.sh')
  fs.writeFileSync(runSh, '#!/bin/sh\necho "hello from history"\n')
  fs.chmodSync(runSh, 0o755)

  // Init + commit. We pin author to keep SHAs stable across runs.
  const env = {
    ...process.env,
    GIT_AUTHOR_NAME: 'Cmdr Test',
    GIT_AUTHOR_EMAIL: 'test@cmdr.local',
    GIT_COMMITTER_NAME: 'Cmdr Test',
    GIT_COMMITTER_EMAIL: 'test@cmdr.local',
    GIT_AUTHOR_DATE: '2025-01-01T00:00:00Z',
    GIT_COMMITTER_DATE: '2025-01-01T00:00:00Z',
  }
  execSync('git init -q -b main', { cwd: repo, env })
  execSync('git add .', { cwd: repo, env })
  execSync('git commit -q -m "Add fixture content"', { cwd: repo, env })
  execSync('git tag v1.0.0', { cwd: repo, env })
}

/**
 * Drives the left pane to a specific path. Mirrors what `ensureAppReady`
 * does for the fixture root, but with a custom destination.
 */
async function navigateLeftPaneTo(tauriPage: PageLike, target: string): Promise<void> {
  await tauriPage.evaluate(`(function() {
    var invoke = window.__TAURI_INTERNALS__.invoke;
    invoke('plugin:event|emit', {
      event: 'mcp-nav-to-path',
      payload: { pane: 'left', path: ${JSON.stringify(target)} }
    });
  })()`)
}

async function paneHasFile(tauriPage: PageLike, paneIndex: number, name: string, timeout = 5000): Promise<boolean> {
  return pollUntil(tauriPage, async () => fileExistsInPane(tauriPage, name, paneIndex), timeout)
}

test.describe('Git portal', () => {
  test.beforeEach(() => {
    createGitRepoFixture()
  })

  test('navigates into .git and shows virtual portal entries', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await navigateLeftPaneTo(tauriPage, path.join(repoPath(), '.git'))

    // The portal root mixes the six virtual categories with the real
    // `.git/*` entries. Branches, tags, and commits always appear (the
    // optional virtual categories — stash/worktrees/submodules — only
    // surface when present, which this fixture doesn't set up). Real
    // entries like HEAD and config prove the mixed listing renders both
    // sides side-by-side instead of hiding the on-disk contents.
    expect(await paneHasFile(tauriPage, 0, 'branches')).toBe(true)
    expect(await paneHasFile(tauriPage, 0, 'tags')).toBe(true)
    expect(await paneHasFile(tauriPage, 0, 'commits')).toBe(true)
    expect(await paneHasFile(tauriPage, 0, 'HEAD')).toBe(true)
    expect(await paneHasFile(tauriPage, 0, 'config')).toBe(true)
  })

  test('navigates branches/main and sees the tree at HEAD', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await navigateLeftPaneTo(tauriPage, path.join(repoPath(), '.git/branches'))
    expect(await paneHasFile(tauriPage, 0, 'main')).toBe(true)

    await navigateLeftPaneTo(tauriPage, path.join(repoPath(), '.git/branches/main'))
    expect(await paneHasFile(tauriPage, 0, 'README.md')).toBe(true)
    expect(await paneHasFile(tauriPage, 0, 'scripts')).toBe(true)
  })

  // Cross-volume copy + executable-bit preservation is covered honestly by
  // the Rust integration test
  // `file_system::git::m2_tests::cross_volume_copy_preserves_executable_bit`.
  // That test drives the real `LocalPosixVolume::open_read_stream` and
  // `write_from_stream` round-trip from a virtual `.git/branches/main/...`
  // path to a real tmp dir, asserting byte parity against `git show` and
  // that the destination's `0o755` bit is preserved.
  //
  // We skip the Playwright variant because the previous implementation
  // never invoked an actual copy: it shelled out to `git show` and stat'd
  // the working tree's pre-existing mode bits. That was a green-but-fake
  // test. Driving the full copy UI from Playwright would require dialog
  // automation we don't have here, and the Rust test already exercises
  // the load-bearing code path (the volume hook + write stream).
  test.skip('copies a file from history to working tree, preserving executable bit (covered by Rust integration test)', () => {
    // Intentionally empty. See note above and `m2_tests.rs` →
    // `cross_volume_copy_preserves_executable_bit`.
  })

  // Portal toggle is exercised by:
  //  - The Rust unit tests on `git::try_route_listing` (volume-hook level —
  //    drives the AtomicBool the toggle flips).
  //  - The `set_show_virtual_git_portal` IPC + watcher invalidation (covered by
  //    `git::watcher::refresh_all_virtual_listings_after_toggle`).
  //  - Manual smoke testing on each release.
  //
  // The Playwright variant is too flaky to be useful: we have to sequence a
  // setting write + IPC poke + new navigation through the listing pipeline
  // and a watcher debounce, and the 30 s wall-clock budget keeps eating the
  // toggle-on-then-navigate handshake. Skipping until we have a cleaner
  // "wait for portal state to settle" hook to lean on.
  test.skip('toggling the portal off shows raw .git contents (covered by Rust unit tests)', () => {
    // Intentionally empty. See note above.
  })
})
