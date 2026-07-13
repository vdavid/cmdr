/**
 * E2E for the M1.5 per-network-volume image-indexing opt-in UI
 * (Settings > Behavior > File system watching > "Image search").
 *
 * Two things proven in the real app + real settings store:
 *  1. The per-volume SMB opt-in PERSISTS: setting `mediaIndex.networkVolumes`
 *     through the same `set_setting` path the toggle uses round-trips into the
 *     sparse store (what's written to `settings.json` and re-read by the Rust
 *     `load_settings` at launch), so it survives a reload. Read back via the
 *     `cmdr://settings` resource, which reflects the live store.
 *  2. The per-network-volume section RENDERS once the master toggle is on.
 *
 * Why not drive a real SMB volume through the switch: SMB volumes only appear
 * under the Docker-backed `smb-e2e` feature (macOS-skipped), so the per-volume
 * switch itself is covered by the component test
 * (`src/lib/settings/sections/MediaIndexNetworkVolumes.a11y.test.ts`, a stubbed
 * network volume) + the persist/rollback unit test
 * (`src/lib/media-index/network-volume-prefs.test.ts`). This spec covers the
 * store round-trip + the real-app render, both verifiable on macOS.
 */

import { test, expect } from './fixtures.js'
import { closeScopedWindow, openSettingsWindowViaProd } from './helpers.js'
import { initMcpClient, mcpCall, mcpReadResource } from '../e2e-shared/mcp-client.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const OPT_IN_SETTING = 'mediaIndex.networkVolumes'
const MASTER_SETTING = 'mediaIndex.enabled'
const STUB_VOLUME_ID = 'e2e-smb-vol'

test.describe('Media index — network volume opt-in', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await initMcpClient(tauriPage)
  })

  test.afterEach(async ({ tauriPage }) => {
    // Restore defaults so the next spec starts clean (sparse store persists these).
    await mcpCall('set_setting', { id: OPT_IN_SETTING, value: [] })
    await mcpCall('set_setting', { id: MASTER_SETTING, value: false })
    await tauriPage.evaluate(`(async function() {
      try {
        var mod = await import('@tauri-apps/api/webviewWindow');
        var win = await mod.WebviewWindow.getByLabel('settings');
        if (win) await win.close();
      } catch (e) { /* ignore */ }
    })()`)
  })

  test('the per-volume SMB opt-in persists in the settings store', async () => {
    // Opt a (stub) network volume in through the exact IPC path the toggle uses.
    await mcpCall('set_setting', { id: OPT_IN_SETTING, value: [STUB_VOLUME_ID] })

    // The store now holds it. `cmdr://settings` reads the live store — the same
    // value the sparse persist writes to settings.json and the backend re-reads.
    const yaml = await mcpReadResource('cmdr://settings')
    expect(yaml).toContain(`id: ${OPT_IN_SETTING}`)
    expect(yaml).toContain(STUB_VOLUME_ID)

    // Clearing it removes the entry (the opt-out path).
    await mcpCall('set_setting', { id: OPT_IN_SETTING, value: [] })
    const cleared = await mcpReadResource('cmdr://settings')
    expect(cleared).not.toContain(STUB_VOLUME_ID)
  })

  test('the network-volume section reveals live once image indexing is on', async ({ tauriPage }) => {
    const settings = await openSettingsWindowViaProd(tauriPage as TauriPage)
    try {
      await settings.waitForSelector('.settings-window', 3000)
      await settings.waitForSelector('.settings-sidebar', 3000)

      const clicked = await settings.evaluate<boolean>(`(function() {
        var items = document.querySelectorAll('.section-item');
        for (var i = 0; i < items.length; i++) {
          if ((items[i].textContent || '').trim() === 'File system watching') {
            items[i].click();
            return true;
          }
        }
        return false;
      })()`)
      expect(clicked).toBe(true)

      // With the master toggle off, the per-network-volume section is hidden.
      const hiddenAtFirst = await settings.evaluate<boolean>(`!document.querySelector('.net-vols')`)
      expect(hiddenAtFirst).toBe(true)

      // Turn on image indexing from the main window; the settings window reveals the
      // section live via the cross-window setting change (no reopen), proving the
      // master-toggle live-apply wiring. No SMB volume is mounted here, so the list
      // shows its "connect a network drive" empty state — `.net-vols` still renders.
      await mcpCall('set_setting', { id: MASTER_SETTING, value: true })
      await settings.waitForSelector('.net-vols', 3000)
      const hasIntro = await settings.evaluate<boolean>(`!!document.querySelector('.net-vols .net-intro')`)
      expect(hasIntro).toBe(true)
    } finally {
      await closeScopedWindow(tauriPage as TauriPage, settings, 'settings')
    }
  })
})
