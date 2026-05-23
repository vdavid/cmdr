/**
 * E2E tests for SMB network integration.
 *
 * Tests virtual SMB host discovery, share listing, mounting, file browsing,
 * and cross-storage copy through the full Cmdr stack: UI → Tauri IPC →
 * network module → Docker SMB containers.
 *
 * Requires:
 * - App built with `--features playwright-e2e,smb-e2e`
 * - Docker SMB containers running: `./apps/desktop/test/smb-servers/start.sh all`
 * - Guest share pre-mounted (handled by smb-fixtures.ts setup)
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import {
  setupSmb,
  teardownSmb,
  SMB_GUEST_MOUNT,
  SMB_GUEST_MOUNT_SUITE,
  SMB_E2E_SUITE_DIR,
  SMB_GUEST_SHARE,
  SMB_GUEST_HOST,
  SMB_GUEST_PORT,
  SMB_AUTH_HOST,
  SMB_AUTH_PORT,
  SMB_AUTH_SHARE,
  SMB_AUTH_USERNAME,
  SMB_AUTH_PASSWORD,
  SMB_50SHARES_HOST,
  SMB_50SHARES_PORT,
  SMB_UNICODE_HOST,
  SMB_UNICODE_PORT,
  smbWriteFile,
} from '../e2e-shared/smb-fixtures.js'
import {
  initMcpClient,
  mcpCall,
  mcpReadResource,
  mcpSelectVolume,
  mcpNavToPath,
  mcpAwaitItem,
  mcpAwaitPath,
} from '../e2e-shared/mcp-client.js'
import { ensureAppReady, getFixtureRoot, pollUntil, sleep, isStateClean } from './helpers.js'

import os from 'os'

// SMB operations involve network + Docker overhead.
test.setTimeout(120_000)

// Linux SMB tests run inside Docker (gvfs-based mounting). mDNS discovery and
// GVFS mount are environmentally flaky on Docker overlay filesystems; see
// e2e-linux/CLAUDE.md for the GVFS / UDisks2VolumeMonitor warning the OS spews
// when concurrent mounts collide. A single retry hides this without masking
// real regressions. SMB tests are skipped on macOS, so the retry only fires on
// Linux.
test.describe.configure({ retries: 1 })

/** Name of the root/local volume (differs by platform). */
const LOCAL_VOLUME_NAME = os.platform() === 'linux' ? 'Root' : 'Macintosh HD'

test.beforeAll(() => {
  setupSmb()
})

test.afterAll(() => {
  teardownSmb()
})

test.beforeEach(async ({ tauriPage }) => {
  // ── MCP health diagnostic ──────────────────────────────────────────────
  // When running the full test suite, the MCP server sometimes dies before
  // SMB tests start. Log the MCP port to help diagnose when/why this happens.
  try {
    const port = await tauriPage.evaluate<number>(`window.__TAURI_INTERNALS__.invoke('get_mcp_port')`)
    // eslint-disable-next-line no-console
    console.log(`[SMB diag] MCP port: ${String(port)}`)
  } catch (err: unknown) {
    // eslint-disable-next-line no-console
    console.error(`[SMB diag] MCP port check failed (app may be dead):`, err instanceof Error ? err.message : err)
  }

  // Fixture recreation is opt-in per test: only the two cross-storage copy
  // tests below touch local files, so the unconditional 1 s watcher settle
  // burned ~14 s across 14 SMB tests that never read from `left/` or `right/`.
  // Those two tests call `recreateFixturesAndSettle()` themselves.

  // Navigate to the main route first: volume-select event listeners
  // only exist on the file explorer page, not on /settings.
  await tauriPage.evaluate(`(function() {
        var a = document.createElement('a');
        a.href = '/';
        document.body.appendChild(a);
        a.click();
        a.remove();
    })()`)
  await tauriPage.waitForSelector('.dual-pane-explorer', 15000)

  await initMcpClient(tauriPage)

  // Force both panes back to a local volume. Previous tests may have left a pane
  // on Network or MTP. ensureAppReady's mcp-nav-to-path events get rejected by
  // navigateToPath when the pane is on a non-local volume, so we switch volumes first.
  //
  // Short-circuit: skip the volume-select + Escape sequence when both panes are
  // already on the local volume and no modal overlay is lingering.
  if (!(await isStateClean(tauriPage, LOCAL_VOLUME_NAME))) {
    await tauriPage.evaluate(`(function() {
          var invoke = window.__TAURI_INTERNALS__.invoke;
          invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'left', name: '${LOCAL_VOLUME_NAME}' } });
          invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'right', name: '${LOCAL_VOLUME_NAME}' } });
      })()`)
    // Wait for both panes to show the local volume in cmdr://state.
    await expect
      .poll(
        async () => {
          const state = await mcpReadResource('cmdr://state')
          const volumeLines = (state.match(/\n {2}volume: ([^\n]+)/g) ?? []).map((line) =>
            line.replace(/^\n {2}volume: /, ''),
          )
          return volumeLines.length >= 2 && volumeLines[0] === LOCAL_VOLUME_NAME && volumeLines[1] === LOCAL_VOLUME_NAME
        },
        { timeout: 5000 },
      )
      .toBeTruthy()

    // Dismiss any lingering dialogs
    await tauriPage.keyboard.press('Escape')
    await tauriPage.keyboard.press('Escape')
    // allowed-bare-poll: best-effort modal dismissal in beforeEach; overlay may or may not be present
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 2000)
  }
})

/**
 * Refresh local `left/`/`right/` fixtures and let the file watcher's initial-scan
 * burst settle before the test reads them. Only the cross-storage copy tests
 * below need this; the rest of the file's tests never read local fixtures.
 *
 * There's no UI-side "watcher armed" signal to poll for (events fire into the
 * backend and are debounced there), so a fixed pre-nav settle is what actually
 * keeps these tests from racing the watcher's first burst.
 */
async function recreateFixturesAndSettle(): Promise<void> {
  recreateFixtures(getFixtureRoot())
  // eslint-disable-next-line cmdr/no-arbitrary-sleep-in-e2e -- watcher initial-scan coalescing window; no UI-side signal, backend debounces watcher events with no observable "armed" marker
  await sleep(1000)
}

// ── Helper ───────────────────────────────────────────────────────────────────

/** Checks whether a host name appears in the network browser's host list. */
async function hostExistsInPane(tauriPage: Parameters<typeof pollUntil>[0], hostName: string): Promise<boolean> {
  return tauriPage.evaluate<boolean>(`(function() {
        var rows = document.querySelectorAll('.host-row .col-name');
        for (var i = 0; i < rows.length; i++) {
            if (rows[i].textContent.indexOf(${JSON.stringify(hostName)}) >= 0) return true;
        }
        return false;
    })()`)
}

/** Checks whether a share name appears in the share browser's share list. */
async function shareExistsInPane(tauriPage: Parameters<typeof pollUntil>[0], shareName: string): Promise<boolean> {
  return tauriPage.evaluate<boolean>(`(function() {
        var rows = document.querySelectorAll('.share-row .share-name');
        for (var i = 0; i < rows.length; i++) {
            if (rows[i].textContent === ${JSON.stringify(shareName)}) return true;
        }
        return false;
    })()`)
}

// ── Tests ────────────────────────────────────────────────────────────────────

// macOS: SMB mounting requires OS-level permission dialogs (mkdir /Volumes/*)
// that can't be approved in headless E2E. Linux uses GVFS mounts which work
// without elevated permissions, so all SMB functionality is tested there.
// eslint-disable-next-line @typescript-eslint/unbound-method -- conditional skip
const describeSmb = process.platform === 'darwin' ? test.describe.skip : test.describe

describeSmb('SMB host discovery', () => {
  test('virtual hosts appear in Network view', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Switch left pane to Network
    await mcpSelectVolume('left', 'Network')

    // Wait for virtual hosts to appear (injected by smb-e2e feature).
    // 30s: defensive bound. Hosts typically appear within 1-3 s; longer budget covers
    // mDNS discovery latency variance on Linux Docker.
    await expect.poll(async () => hostExistsInPane(tauriPage, 'SMB Test (Guest)'), { timeout: 30000 }).toBeTruthy()

    const hasGuest = await hostExistsInPane(tauriPage, 'SMB Test (Guest)')
    const hasAuth = await hostExistsInPane(tauriPage, 'SMB Test (Auth)')
    expect(hasGuest).toBe(true)
    expect(hasAuth).toBe(true)
  })

  test('guest host shows share count after discovery', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    await mcpSelectVolume('left', 'Network')

    // Wait for the guest host to appear and its shares to be prefetched
    await expect
      .poll(
        async () => {
          const state = await mcpReadResource('cmdr://state')
          return state.includes('SMB Test (Guest)') && state.includes('shares=1')
        },
        { timeout: 30000 },
      )
      .toBeTruthy()
  })
})

describeSmb('SMB share browsing', () => {
  test('opening guest host shows share list with public share', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Switch to Network, wait for hosts
    await mcpSelectVolume('left', 'Network')
    await expect.poll(async () => hostExistsInPane(tauriPage, 'SMB Test (Guest)'), { timeout: 15000 }).toBeTruthy()

    // Move cursor to guest host and open it
    await mcpCall('move_cursor', { pane: 'left', filename: 'SMB Test (Guest)' })
    await mcpCall('open_under_cursor', {})

    // Wait for share browser to load (look for .share-row elements)
    await expect.poll(async () => shareExistsInPane(tauriPage, SMB_GUEST_SHARE), { timeout: 30000 }).toBeTruthy()

    const hasPublic = await shareExistsInPane(tauriPage, SMB_GUEST_SHARE)
    expect(hasPublic).toBe(true)
  })
})

describeSmb('SMB mounting and file browsing', () => {
  test('mounting guest share navigates to mounted path', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // The guest share is pre-mounted by smb-fixtures.ts setupSmb().
    // When the app tries to mount it, it gets EEXIST (already mounted) which is treated as success.

    // Switch to Network → open guest host → select share
    await mcpSelectVolume('left', 'Network')
    await expect.poll(async () => hostExistsInPane(tauriPage, 'SMB Test (Guest)'), { timeout: 15000 }).toBeTruthy()

    await mcpCall('move_cursor', { pane: 'left', filename: 'SMB Test (Guest)' })
    await mcpCall('open_under_cursor', {})
    await expect.poll(async () => shareExistsInPane(tauriPage, SMB_GUEST_SHARE), { timeout: 30000 }).toBeTruthy()

    // Open the share (triggers mount)
    await mcpCall('move_cursor', { pane: 'left', filename: SMB_GUEST_SHARE })
    await mcpCall('open_under_cursor', {})

    // After mounting, the pane should navigate to the mounted volume path.
    await mcpAwaitPath('left', SMB_GUEST_MOUNT, 30)

    const state = await mcpReadResource('cmdr://state')
    expect(state).toContain(SMB_GUEST_MOUNT)
  })

  test('browse files on mounted guest share', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Navigate directly to the pre-mounted share
    // (The share is mounted by setupSmb → preMountGuestShare)
    if (!fs.existsSync(SMB_GUEST_MOUNT)) {
      test.skip()
      return
    }

    await mcpNavToPath('left', SMB_GUEST_MOUNT)

    // The Docker guest container creates files in /share.
    // Verify we can read the directory (even if empty, the navigation should succeed).
    const state = await mcpReadResource('cmdr://state')
    expect(state).toContain(SMB_GUEST_MOUNT)
  })
})

describeSmb('SMB cross-storage copy', () => {
  test('copies file from local to mounted SMB share', async ({ tauriPage }) => {
    await recreateFixturesAndSettle()
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    if (!fs.existsSync(SMB_GUEST_MOUNT_SUITE)) {
      test.skip()
      return
    }

    // Left pane: local fixtures (already set by ensureAppReady)
    // Right pane: suite-specific subdir on the mounted SMB share
    // (write isolation from the Rust integration tests — see
    // SMB_E2E_SUITE_DIR in smb-fixtures.ts).
    await mcpNavToPath('right', SMB_GUEST_MOUNT_SUITE)

    // Copy file-a.txt from left to right
    await mcpCall('move_cursor', { pane: 'left', filename: 'file-a.txt' })
    await mcpCall('copy', { autoConfirm: true })

    // Wait for the file to appear on the SMB share
    await mcpAwaitItem('right', 'file-a.txt', 30)

    // Verify on disk (the mount maps to Docker container volume)
    const copied = path.join(SMB_GUEST_MOUNT_SUITE, 'file-a.txt')
    await expect.poll(() => fs.existsSync(copied), { timeout: 10000 }).toBeTruthy()

    // Verify source still exists (copy, not move)
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(true)
  })

  test('copies file from mounted SMB share to local', async ({ tauriPage }) => {
    await recreateFixturesAndSettle()
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    if (!fs.existsSync(SMB_GUEST_MOUNT_SUITE)) {
      test.skip()
      return
    }

    // Write test file directly to the SMB server via smbclient (bypasses GVFS
    // caching; files written through the GVFS mount aren't immediately visible).
    // Target the suite-specific subdir so concurrent Rust integration tests
    // can never see this file.
    smbWriteFile(
      SMB_GUEST_HOST,
      SMB_GUEST_PORT,
      SMB_GUEST_SHARE,
      `${SMB_E2E_SUITE_DIR}/smb-test-file.txt`,
      'File from SMB share for E2E test.\n',
    )

    // Left pane: SMB share, Right pane: local fixtures right/
    await mcpNavToPath('left', SMB_GUEST_MOUNT_SUITE)
    await mcpAwaitItem('left', 'smb-test-file.txt', 15)

    // Copy from SMB to local
    await mcpCall('move_cursor', { pane: 'left', filename: 'smb-test-file.txt' })
    await mcpCall('copy', { autoConfirm: true })

    // Wait for file to appear in right pane
    await mcpAwaitItem('right', 'smb-test-file.txt', 30)

    // Verify on disk
    const localCopy = path.join(fixtureRoot, 'right', 'smb-test-file.txt')
    expect(fs.existsSync(localCopy)).toBe(true)
    expect(fs.readFileSync(localCopy, 'utf-8')).toContain('File from SMB share')
  })
})

// ── Authentication tests ─────────────────────────────────────────────────────
//
// The auth Docker container (smb-auth, `guest ok = no`) allows guest share
// LISTING via IPC$ (Samba default: `map to guest = bad user`). Only share
// ACCESS (mounting) requires credentials. The ShareBrowser shows the share
// list directly without a login prompt when opening the auth host.
//
// These tests verify the auth host's share discovery and the share listing
// via IPC with credentials (the backend path used by the login form).

describeSmb('SMB authentication', () => {
  test('auth host shows share count after discovery', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    await mcpSelectVolume('left', 'Network')

    // Wait for the auth host to appear and its shares to be prefetched
    await expect
      .poll(
        async () => {
          const state = await mcpReadResource('cmdr://state')
          return state.includes('SMB Test (Auth)') && state.includes('shares=1')
        },
        { timeout: 30000 },
      )
      .toBeTruthy()
  })

  test('listing shares with valid credentials returns private share', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Call the Tauri IPC command directly: same backend path the login form
    // uses. Uses a unique hostId to bypass any cached results.
    const result = await tauriPage.evaluate<{ shares: { name: string }[]; authMode: string }>(`
      window.__TAURI_INTERNALS__.invoke('list_shares_with_credentials', {
        hostId: 'smb-e2e-auth-valid-' + Date.now(),
        hostname: ${JSON.stringify(SMB_AUTH_HOST)},
        ipAddress: undefined,
        port: ${String(SMB_AUTH_PORT)},
        username: ${JSON.stringify(SMB_AUTH_USERNAME)},
        password: ${JSON.stringify(SMB_AUTH_PASSWORD)},
        timeoutMs: 30000,
        cacheTtlMs: 5000,
      })
    `)

    const shareNames = result.shares.map((s) => s.name)
    expect(shareNames).toContain(SMB_AUTH_SHARE)
  })
})

// ── Diverse server tests ────────────────────────────────────────────────────
//
// These tests exercise Cmdr's UI against smb2's consumer containers with
// non-trivial data: many shares, unicode names, etc. They test discovery
// and share listing (no mounting needed).

describeSmb('SMB 50-share server', () => {
  test('50-share server lists all shares', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // List shares via IPC (bypasses UI, tests the backend share listing path)
    const result = await tauriPage.evaluate<{ shares: { name: string }[] }>(`
      window.__TAURI_INTERNALS__.invoke('list_shares_with_credentials', {
        hostId: 'smb-e2e-50shares-' + Date.now(),
        hostname: ${JSON.stringify(SMB_50SHARES_HOST)},
        ipAddress: undefined,
        port: ${String(SMB_50SHARES_PORT)},
        username: '',
        password: '',
        timeoutMs: 30000,
        cacheTtlMs: 5000,
      })
    `)

    // smb2's consumer 50-shares container creates 50 shares
    expect(result.shares.length).toBeGreaterThanOrEqual(50)
  })

  test('50-share host shows correct share count in Network view', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    await mcpSelectVolume('left', 'Network')

    // Wait for the 50-shares host to appear and prefetch shares
    await expect
      .poll(
        async () => {
          const state = await mcpReadResource('cmdr://state')
          return state.includes('SMB Test (50 Shares)') && state.includes('shares=50')
        },
        { timeout: 30000 },
      )
      .toBeTruthy()
  })
})

describeSmb('SMB unicode server', () => {
  test('unicode server lists shares with non-ASCII names', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // List shares via IPC
    const result = await tauriPage.evaluate<{ shares: { name: string }[] }>(`
      window.__TAURI_INTERNALS__.invoke('list_shares_with_credentials', {
        hostId: 'smb-e2e-unicode-' + Date.now(),
        hostname: ${JSON.stringify(SMB_UNICODE_HOST)},
        ipAddress: undefined,
        port: ${String(SMB_UNICODE_PORT)},
        username: '',
        password: '',
        timeoutMs: 30000,
        cacheTtlMs: 5000,
      })
    `)

    // smb2's unicode container has shares with CJK, emoji, and accented names
    expect(result.shares.length).toBeGreaterThan(0)

    // At least one share name should contain non-ASCII characters
    const hasNonAscii = result.shares.some((s) => s.name.split('').some((c) => c.charCodeAt(0) > 127))
    expect(hasNonAscii).toBe(true)
  })

  test('unicode shares render correctly in share browser', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Switch to Network, open unicode host
    await mcpSelectVolume('left', 'Network')
    await expect.poll(async () => hostExistsInPane(tauriPage, 'SMB Test (Unicode)'), { timeout: 15000 }).toBeTruthy()

    await mcpCall('move_cursor', { pane: 'left', filename: 'SMB Test (Unicode)' })
    await mcpCall('open_under_cursor', {})

    // Wait for share browser to load, should show at least one share
    await expect
      .poll(
        async () => {
          return tauriPage.evaluate<boolean>(`(function() {
          var rows = document.querySelectorAll('.share-row .share-name');
          return rows.length > 0;
        })()`)
        },
        { timeout: 30000 },
      )
      .toBeTruthy()

    // Verify share names rendered (not empty or garbled)
    const shareNames = await tauriPage.evaluate<string[]>(`(function() {
      var rows = document.querySelectorAll('.share-row .share-name');
      var names = [];
      for (var i = 0; i < rows.length; i++) {
        names.push(rows[i].textContent);
      }
      return names;
    })()`)

    expect(shareNames.length).toBeGreaterThan(0)
    // Names should not be empty strings (garbled rendering)
    expect(shareNames.every((n) => n.length > 0)).toBe(true)
  })
})
