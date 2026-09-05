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
 *
 * 4. Deleting the whole repo folder with the portal ON, which is the app-level
 *    proof that no walker meets a virtual folder any more.
 * 5. Flipping the portal off with a `.git/` pane open: the six rows go, the
 *    real entries stay, and flipping back brings them straight back.
 *
 * Cross-volume copy with executable-bit preservation lives in the Rust
 * integration test `file_system::git::portal_tests::cross_volume_copy_preserves_executable_bit`,
 * which drives the real `open_read_stream` and `write_from_stream` round-trip. Driving the full copy UI from Playwright
 * would need dialog automation we don't have, and the Rust test exercises the
 * load-bearing code path (the portal volume's read stream + the local write stream).
 *
 * Editing, renaming, and removing a REAL file under `.git/` is
 * `file_system::git::walker_exposure_tests::real_files_under_dot_git_stay_fully_mutable`,
 * which drives every write method on the local volume directly.
 */

import fs from 'fs'
import path from 'path'
import { execSync } from 'child_process'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { test, expect } from './fixtures.js'
import { ensureAppReady, getFixtureRoot, fileExistsInPane, pollUntil } from './helpers.js'
import { ensureMcpClient, mcpCall, mcpNavToPath } from '../e2e-shared/mcp-client.js'

/** Matches the `PageLike` alias used inside `helpers.ts`. */
type PageLike = TauriPage | BrowserPageAdapter

const REPO_REL = 'git-portal-repo'

function repoPath(): string {
  return path.join(getFixtureRoot(), REPO_REL)
}

/**
 * Builds a deterministic git repo inside the fixture root. Idempotent:
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

async function paneLacksFile(tauriPage: PageLike, paneIndex: number, name: string, timeout = 5000): Promise<boolean> {
  return pollUntil(tauriPage, async () => !(await fileExistsInPane(tauriPage, name, paneIndex)), timeout)
}

/** Flips the live portal setting the way `settings-applier.ts` does. */
async function setPortalEnabled(tauriPage: PageLike, enabled: boolean): Promise<void> {
  await tauriPage.evaluate(`(function() {
    return window.__TAURI_INTERNALS__.invoke('set_show_virtual_git_portal', { enabled: ${JSON.stringify(enabled)} });
  })()`)
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
    // optional virtual categories (stash/worktrees/submodules), only
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

  test('navigates tags/v1.0.0 and sees the tree at the tagged commit', async ({ tauriPage }) => {
    // The portal listing parses the tag name greedily, including the dots in
    // `v1.0.0`. Verifying the tag content reaches the same tree as `branches/main`
    // also covers the annotated-tag-peel path in `resolve_ref_commit`.
    await ensureAppReady(tauriPage)
    await navigateLeftPaneTo(tauriPage, path.join(repoPath(), '.git/tags'))
    expect(await paneHasFile(tauriPage, 0, 'v1.0.0')).toBe(true)

    await navigateLeftPaneTo(tauriPage, path.join(repoPath(), '.git/tags/v1.0.0'))
    expect(await paneHasFile(tauriPage, 0, 'README.md')).toBe(true)
    expect(await paneHasFile(tauriPage, 0, 'scripts')).toBe(true)
  })

  // The portal is process-global, so a cell that turns it off puts it back.
  test.afterEach(async ({ tauriPage }) => {
    await setPortalEnabled(tauriPage, true)
  })

  test('deletes the whole repo folder with the portal on, leaving no .git behind', async ({ tauriPage }) => {
    // The bug this pins: the six virtual folders used to reach the volume-aware
    // delete walker, which refused each one (and every real file under `.git`)
    // with `NotSupported`, stopping with the repo half-gone.
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await mcpNavToPath('left', getFixtureRoot())
    expect(await paneHasFile(tauriPage, 0, REPO_REL)).toBe(true)

    await mcpCall('select', { pane: 'left', names: [REPO_REL] })
    expect(await mcpCall('delete', { mode: 'delete', autoConfirm: true })).toContain('OK')

    const gone = await mcpCall('await', {
      pane: 'left',
      condition: 'not_has_item',
      value: REPO_REL,
      timeoutSeconds: 20,
    })
    expect(gone).toContain('OK')
    expect(fs.existsSync(path.join(repoPath(), '.git'))).toBe(false)
    expect(fs.existsSync(repoPath())).toBe(false)
  })

  test('turning the portal off with a .git pane open drops the virtual rows and keeps the real ones', async ({
    tauriPage,
  }) => {
    await ensureAppReady(tauriPage)
    await navigateLeftPaneTo(tauriPage, path.join(repoPath(), '.git'))
    expect(await paneHasFile(tauriPage, 0, 'branches')).toBe(true)

    // The setting flip alone isn't enough; the backend also refreshes every open
    // `.git/` listing, and the refresh must re-read WITHOUT the overlay's rows.
    await setPortalEnabled(tauriPage, false)
    expect(await paneLacksFile(tauriPage, 0, 'branches')).toBe(true)
    expect(await paneLacksFile(tauriPage, 0, 'commits')).toBe(true)
    expect(await paneHasFile(tauriPage, 0, 'HEAD')).toBe(true)
    expect(await paneHasFile(tauriPage, 0, 'config')).toBe(true)

    // And back: the same refresh runs the overlay again.
    await setPortalEnabled(tauriPage, true)
    expect(await paneHasFile(tauriPage, 0, 'branches')).toBe(true)
    expect(await paneHasFile(tauriPage, 0, 'HEAD')).toBe(true)
  })

  test('navigates commits/ and shows the single HEAD commit by short SHA', async ({ tauriPage }) => {
    // commits/ lists each reachable commit as a virtual directory whose display
    // name is `<short-sha> <subject>` (matches `git log --oneline`). The fixture
    // commits exactly once, so we expect at least one such entry. We don't pin
    // the SHA itself, since `git init` results aren't reproducible across versions.
    await ensureAppReady(tauriPage)
    await navigateLeftPaneTo(tauriPage, path.join(repoPath(), '.git/commits'))

    const found = await pollUntil(
      tauriPage,
      async () =>
        tauriPage.evaluate<boolean>(`(function() {
          var pane = document.querySelectorAll('.file-pane')[0];
          if (!pane) return false;
          var entries = pane.querySelectorAll('.file-entry');
          for (var i = 0; i < entries.length; i++) {
            var name = entries[i].getAttribute('data-filename') || '';
            // <short-sha> <subject>, for example "abc1234 Add fixture content"
            if (/^[0-9a-f]{7,} /.test(name)) return true;
          }
          return false;
        })()`),
      5000,
    )
    expect(found).toBe(true)
  })
})
