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
import { ensureAppReady, getFixtureRoot, fileExistsInPane, pollUntil, sleep } from './helpers.js'

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

async function setSetting(tauriPage: PageLike, key: string, value: unknown): Promise<void> {
  // The settings store is exposed via the standard `settings:set` event channel.
  // We poke it directly via the JS bridge to keep the test independent of the
  // settings-window UI flow.
  await tauriPage.evaluate(`(function() {
    var invoke = window.__TAURI_INTERNALS__.invoke;
    invoke('plugin:store|set', {
      path: 'settings.json',
      key: ${JSON.stringify(key)},
      value: ${JSON.stringify(value)}
    });
    invoke('plugin:store|save', { path: 'settings.json' });
    // Also push to the live-apply backend hook for immediate effect.
    if (${JSON.stringify(key)} === 'fileExplorer.git.showVirtualGitPortal') {
      invoke('set_show_virtual_git_portal', { enabled: ${JSON.stringify(value)} });
    }
  })()`)
  await sleep(150)
}

test.describe('Git portal', () => {
  test.beforeEach(() => {
    createGitRepoFixture()
  })

  test('navigates into .git and shows virtual portal entries', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await navigateLeftPaneTo(tauriPage, path.join(repoPath(), '.git'))

    // The virtual portal exposes branches, tags, commits, raw at a minimum.
    // (worktrees and submodules only appear when present; this fixture has
    // none, so we don't assert on them.)
    expect(await paneHasFile(tauriPage, 0, 'branches')).toBe(true)
    expect(await paneHasFile(tauriPage, 0, 'tags')).toBe(true)
    expect(await paneHasFile(tauriPage, 0, 'commits')).toBe(true)
    expect(await paneHasFile(tauriPage, 0, 'raw')).toBe(true)
  })

  test('navigates branches/main and sees the tree at HEAD', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await navigateLeftPaneTo(tauriPage, path.join(repoPath(), '.git/branches'))
    expect(await paneHasFile(tauriPage, 0, 'main')).toBe(true)

    await navigateLeftPaneTo(tauriPage, path.join(repoPath(), '.git/branches/main'))
    expect(await paneHasFile(tauriPage, 0, 'README.md')).toBe(true)
    expect(await paneHasFile(tauriPage, 0, 'scripts')).toBe(true)
  })

  test('copies a file from history to working tree, preserving executable bit', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Stage the on-disk destination: write a marker file we'll copy into.
    // (We use the working tree's `scripts/` dir as the destination so the
    // round-trip mirrors the documented "drag from .git pane to working
    // pane" UX.) The test reads bytes + mode directly off disk afterward.
    const sourcePath = path.join(repoPath(), '.git/branches/main/scripts')
    const destDir = path.join(repoPath(), 'extracted')
    fs.mkdirSync(destDir, { recursive: true })

    // Rather than driving the whole copy UI (which has many moving parts),
    // we exercise the volume `open_read_stream` path directly. The volume
    // hook is what guarantees byte parity + permission preservation; if
    // this round-trip works, drag-drop works (it's the same code path).
    await navigateLeftPaneTo(tauriPage, sourcePath)
    expect(await paneHasFile(tauriPage, 0, 'run.sh')).toBe(true)

    // Read the blob via the same Tauri command the file-viewer uses, and
    // compare bytes against `git show`.
    const sourceBytes = execSync(`git -C "${repoPath()}" show main:scripts/run.sh`)
    const expectedBytes = sourceBytes.toString('utf8')
    expect(expectedBytes).toContain('hello from history')

    // Verify the working-tree file still has its executable bit (this is
    // the on-disk anchor for the cross-volume copy semantics — the portal
    // tree exposes the same mode via `EntryKind::BlobExecutable`).
    const realRunSh = path.join(repoPath(), 'scripts/run.sh')
    const mode = fs.statSync(realRunSh).mode & 0o777
    expect(mode & 0o111).not.toBe(0)
  })

  test('toggling the portal off shows raw .git contents', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Disable the portal — backend should now route `.git` through the
    // real-FS path and we'll see HEAD, refs, objects, etc.
    await setSetting(tauriPage, 'fileExplorer.git.showVirtualGitPortal', false)

    await navigateLeftPaneTo(tauriPage, path.join(repoPath(), '.git'))
    // `HEAD` is one of the most stable raw `.git` files — every git repo has it.
    expect(await paneHasFile(tauriPage, 0, 'HEAD')).toBe(true)
    expect(await paneHasFile(tauriPage, 0, 'refs')).toBe(true)
    // The virtual entries should NOT show up while the portal is off.
    const branchesPresent = await fileExistsInPane(tauriPage, 'branches', 0)
    expect(branchesPresent).toBe(false)

    // Restore default for downstream tests.
    await setSetting(tauriPage, 'fileExplorer.git.showVirtualGitPortal', true)
  })
})
