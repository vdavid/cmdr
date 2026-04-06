/**
 * E2E tests for SMB network integration.
 *
 * Tests virtual SMB host discovery, share listing, mounting, file browsing,
 * and cross-storage copy through the full Cmdr stack: UI → Tauri IPC →
 * network module → Docker SMB containers.
 *
 * Requires:
 * - App built with `--features playwright-e2e,smb-e2e`
 * - Docker SMB containers running: `./apps/desktop/test/smb-servers/start.sh minimal`
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
  SMB_GUEST_SHARE,
  SMB_GUEST_HOST,
  SMB_GUEST_PORT,
  SMB_AUTH_HOST,
  SMB_AUTH_PORT,
  SMB_AUTH_SHARE,
  SMB_AUTH_USERNAME,
  SMB_AUTH_PASSWORD,
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
import { ensureAppReady, getFixtureRoot, pollUntil, sleep } from './helpers.js'

import os from 'os'

// SMB operations involve network + Docker overhead.
test.setTimeout(120_000)

/** Name of the root/local volume — differs by platform. */
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

  recreateFixtures(getFixtureRoot())
  await sleep(1000) // Let file watcher settle after fixture recreation

  // Navigate to the main route first — volume-select event listeners
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
  await tauriPage.evaluate(`(function() {
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'left', name: '${LOCAL_VOLUME_NAME}' } });
        invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'right', name: '${LOCAL_VOLUME_NAME}' } });
    })()`)
  await sleep(2000)

  // Dismiss any lingering dialogs
  await tauriPage.keyboard.press('Escape')
  await sleep(200)
  await tauriPage.keyboard.press('Escape')
  await sleep(200)
})

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

test.describe('SMB host discovery', () => {
  test('virtual hosts appear in Network view', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Switch left pane to Network
    await mcpSelectVolume('left', 'Network')
    await sleep(2000)

    // Wait for virtual hosts to appear (injected by smb-e2e feature)
    await pollUntil(tauriPage, async () => hostExistsInPane(tauriPage, 'SMB Test (Guest)'), 15000)

    const hasGuest = await hostExistsInPane(tauriPage, 'SMB Test (Guest)')
    const hasAuth = await hostExistsInPane(tauriPage, 'SMB Test (Auth)')
    expect(hasGuest).toBe(true)
    expect(hasAuth).toBe(true)
  })

  test('guest host shows share count after discovery', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    await mcpSelectVolume('left', 'Network')
    await sleep(2000)

    // Wait for the guest host to appear and its shares to be prefetched
    await pollUntil(
      tauriPage,
      async () => {
        const state = await mcpReadResource('cmdr://state')
        return state.includes('SMB Test (Guest)') && state.includes('shares=1')
      },
      30000,
    )
  })
})

test.describe('SMB share browsing', () => {
  test('opening guest host shows share list with public share', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Switch to Network, wait for hosts
    await mcpSelectVolume('left', 'Network')
    await sleep(2000)
    await pollUntil(tauriPage, async () => hostExistsInPane(tauriPage, 'SMB Test (Guest)'), 15000)

    // Move cursor to guest host and open it
    await mcpCall('move_cursor', { pane: 'left', filename: 'SMB Test (Guest)' })
    await mcpCall('open_under_cursor', {})

    // Wait for share browser to load — look for .share-row elements
    await pollUntil(tauriPage, async () => shareExistsInPane(tauriPage, SMB_GUEST_SHARE), 30000)

    const hasPublic = await shareExistsInPane(tauriPage, SMB_GUEST_SHARE)
    expect(hasPublic).toBe(true)
  })
})

test.describe('SMB mounting and file browsing', () => {
  test('mounting guest share navigates to mounted path', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // The guest share is pre-mounted by smb-fixtures.ts setupSmb().
    // When the app tries to mount it, it gets EEXIST (already mounted) which is treated as success.

    // Switch to Network → open guest host → select share
    await mcpSelectVolume('left', 'Network')
    await sleep(2000)
    await pollUntil(tauriPage, async () => hostExistsInPane(tauriPage, 'SMB Test (Guest)'), 15000)

    await mcpCall('move_cursor', { pane: 'left', filename: 'SMB Test (Guest)' })
    await mcpCall('open_under_cursor', {})
    await pollUntil(tauriPage, async () => shareExistsInPane(tauriPage, SMB_GUEST_SHARE), 30000)

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
    await sleep(2000)

    // The Docker guest container creates files in /share.
    // Verify we can read the directory (even if empty, the navigation should succeed).
    const state = await mcpReadResource('cmdr://state')
    expect(state).toContain(SMB_GUEST_MOUNT)
  })
})

test.describe('SMB cross-storage copy', () => {
  test('copies file from local to mounted SMB share', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    if (!fs.existsSync(SMB_GUEST_MOUNT)) {
      test.skip()
      return
    }

    // Left pane: local fixtures (already set by ensureAppReady)
    // Right pane: mounted SMB share
    await mcpNavToPath('right', SMB_GUEST_MOUNT)
    await sleep(2000)

    // Copy file-a.txt from left to right
    await mcpCall('move_cursor', { pane: 'left', filename: 'file-a.txt' })
    await mcpCall('copy', { autoConfirm: true })

    // Wait for the file to appear on the SMB share
    await mcpAwaitItem('right', 'file-a.txt', 30)

    // Verify on disk (the mount maps to Docker container volume)
    const copied = path.join(SMB_GUEST_MOUNT, 'file-a.txt')
    await pollUntil(tauriPage, () => Promise.resolve(fs.existsSync(copied)), 10000)
    expect(fs.existsSync(copied)).toBe(true)

    // Verify source still exists (copy, not move)
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'file-a.txt'))).toBe(true)
  })

  test('copies file from mounted SMB share to local', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    if (!fs.existsSync(SMB_GUEST_MOUNT)) {
      test.skip()
      return
    }

    // Write test file directly to the SMB server via smbclient (bypasses GVFS
    // caching — files written through the GVFS mount aren't immediately visible).
    smbWriteFile(
      SMB_GUEST_HOST,
      SMB_GUEST_PORT,
      SMB_GUEST_SHARE,
      'smb-test-file.txt',
      'File from SMB share for E2E test.\n',
    )

    // Left pane: SMB share, Right pane: local fixtures right/
    await mcpNavToPath('left', SMB_GUEST_MOUNT)
    await sleep(2000)
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

test.describe('SMB authentication', () => {
  test('auth host shows share count after discovery', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    await mcpSelectVolume('left', 'Network')
    await sleep(2000)

    // Wait for the auth host to appear and its shares to be prefetched
    await pollUntil(
      tauriPage,
      async () => {
        const state = await mcpReadResource('cmdr://state')
        return state.includes('SMB Test (Auth)') && state.includes('shares=1')
      },
      30000,
    )
  })

  test('listing shares with valid credentials returns private share', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Call the Tauri IPC command directly — same backend path the login form
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
