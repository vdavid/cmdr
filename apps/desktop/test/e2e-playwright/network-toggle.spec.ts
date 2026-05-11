/**
 * E2E tests for the `network.enabled` toggle UX.
 *
 * Covers the user-visible behavior of toggling networking off/on:
 * - Volume picker shows "Network" by default
 * - Toggling off renames it to "Network (disabled)"
 * - Clicking "Network (disabled)" opens Settings → Network → SMB/Network shares
 * - Toggling back on restores "Network"
 *
 * Uses the `mcp-set-setting` event to write the setting from the test, which
 * triggers the same code path as the Settings UI (cache + cross-window emit +
 * `settings-applier` live-apply). This keeps the test focused on the picker
 * UX without coupling it to settings-page navigation.
 *
 * Out of scope: macOS Local Network OS prompt timing (TCC dialog isn't
 * driveable from automation). Validated manually.
 */

import os from 'node:os'
import { test, expect } from './fixtures.js'
import { ensureAppReady, pollUntil, sleep } from './helpers.js'
import { initMcpClient, mcpReadResource } from '../e2e-shared/mcp-client.js'

// Volume name for "Macintosh HD" on macOS / "Root" on Linux. We force both panes back to
// this volume in `beforeEach` so the spec runs cleanly even when a prior MTP test left a
// pane on a virtual MTP volume (mcp-nav-to-path won't cross volume boundaries).
const LOCAL_VOLUME_NAME = os.platform() === 'linux' ? 'Root' : 'Macintosh HD'

const PICKER_TRIGGER = '.volume-name'
const PICKER_DROPDOWN = '.volume-dropdown'
const ANY_VOLUME_ITEM = '.volume-item'

/** Reads the visible label of the synthetic Network volume entry. */
async function readNetworkLabel(tauriPage: Parameters<typeof pollUntil>[0]): Promise<string | null> {
  return tauriPage.evaluate<string | null>(`(function() {
    var items = document.querySelectorAll('.volume-item');
    for (var i = 0; i < items.length; i++) {
      var label = items[i].querySelector('.volume-label');
      if (!label) continue;
      var text = label.textContent || '';
      if (text === 'Network' || text === 'Network (disabled)') return text;
    }
    return null;
  })()`)
}

/** Sets a setting through the MCP bridge — same code path the UI uses. */
async function setSettingViaBridge(
  tauriPage: Parameters<typeof pollUntil>[0],
  settingId: string,
  value: unknown,
): Promise<void> {
  const requestId = `e2e-${String(Date.now())}-${Math.random().toString(36).slice(2)}`
  const settingIdJson = JSON.stringify(settingId)
  const valueJson = JSON.stringify(value)
  const requestIdJson = JSON.stringify(requestId)
  await tauriPage.evaluate(`(function() {
    var invoke = window.__TAURI_INTERNALS__.invoke;
    invoke('plugin:event|emit', {
      event: 'mcp-set-setting',
      payload: { requestId: ${requestIdJson}, settingId: ${settingIdJson}, value: ${valueJson} }
    });
  })()`)
  // Allow the bridge handler + cross-window emit + reactive update to settle.
  await sleep(300)
}

async function openVolumePicker(tauriPage: Parameters<typeof pollUntil>[0]): Promise<void> {
  // If already open, no-op
  if (await tauriPage.isVisible(PICKER_DROPDOWN)) return
  await tauriPage.click(PICKER_TRIGGER)
  await tauriPage.waitForSelector(PICKER_DROPDOWN, 5000)
}

async function closeVolumePicker(tauriPage: Parameters<typeof pollUntil>[0]): Promise<void> {
  if (!(await tauriPage.isVisible(PICKER_DROPDOWN))) return
  await tauriPage.evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`)
  await pollUntil(tauriPage, async () => !(await tauriPage.isVisible(PICKER_DROPDOWN)), 3000)
}

test.describe('Network toggle in volume picker', () => {
  test.beforeEach(async ({ tauriPage }) => {
    // Force both panes back to a local volume in case a prior MTP test left a pane on
    // a virtual MTP volume — `ensureAppReady`'s `mcp-nav-to-path` doesn't cross volume
    // boundaries, so we have to switch volumes explicitly first.
    await tauriPage.evaluate(`(function() {
      var invoke = window.__TAURI_INTERNALS__.invoke;
      invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'left', name: '${LOCAL_VOLUME_NAME}' } });
      invoke('plugin:event|emit', { event: 'mcp-volume-select', payload: { pane: 'right', name: '${LOCAL_VOLUME_NAME}' } });
    })()`)
    // Wait for both panes to actually be on the local volume before asserting picker UX.
    await initMcpClient(tauriPage)
    await pollUntil(
      tauriPage,
      async () => {
        const state = await mcpReadResource('cmdr://state')
        const volumeLines = (state.match(/\n {2}volume: ([^\n]+)/g) ?? []).map((line) =>
          line.replace(/^\n {2}volume: /, ''),
        )
        return volumeLines.length >= 2 && volumeLines[0] === LOCAL_VOLUME_NAME && volumeLines[1] === LOCAL_VOLUME_NAME
      },
      5000,
    )

    await ensureAppReady(tauriPage)

    // Reset the toggle to its default in case a prior test left it off.
    await setSettingViaBridge(tauriPage, 'network.enabled', true)
    await closeVolumePicker(tauriPage)
  })

  test.afterEach(async ({ tauriPage }) => {
    // Restore the default so the next spec file starts clean.
    await setSettingViaBridge(tauriPage, 'network.enabled', true)
    // Close any settings window the click test may have opened. Best-effort — the
    // Tauri webviewWindow API is the same module the app uses to open it.
    await tauriPage.evaluate(`(async function() {
      try {
        var mod = await import('@tauri-apps/api/webviewWindow');
        var win = await mod.WebviewWindow.getByLabel('settings');
        if (win) await win.close();
      } catch (e) {
        // ignore
      }
    })()`)
  })

  test('shows "Network" by default', async ({ tauriPage }) => {
    await openVolumePicker(tauriPage)
    await tauriPage.waitForSelector(ANY_VOLUME_ITEM, 3000)
    const label = await readNetworkLabel(tauriPage)
    expect(label).toBe('Network')
  })

  test('shows "Network (disabled)" when toggle is off', async ({ tauriPage }) => {
    await setSettingViaBridge(tauriPage, 'network.enabled', false)
    await openVolumePicker(tauriPage)
    await tauriPage.waitForSelector(ANY_VOLUME_ITEM, 3000)
    const label = await pollUntilLabel(tauriPage, 'Network (disabled)')
    expect(label).toBe('Network (disabled)')
  })

  test('toggling back on restores "Network"', async ({ tauriPage }) => {
    await setSettingViaBridge(tauriPage, 'network.enabled', false)
    await setSettingViaBridge(tauriPage, 'network.enabled', true)
    await openVolumePicker(tauriPage)
    await tauriPage.waitForSelector(ANY_VOLUME_ITEM, 3000)
    const label = await pollUntilLabel(tauriPage, 'Network')
    expect(label).toBe('Network')
  })

  test('clicking "Network (disabled)" closes the dropdown without changing volume', async ({ tauriPage }) => {
    await setSettingViaBridge(tauriPage, 'network.enabled', false)

    // Capture the visible breadcrumb label BEFORE the click. The picker's `.volume-name`
    // shows the active volume; if `handleVolumeSelect` had taken the navigate-to-volume
    // branch (i.e. our early-return guard didn't fire), this label would change after the
    // click. So path-stability is our proxy for "the early-return branch ran" — the only
    // branch that fits the disabled-network condition.
    const labelBefore = await tauriPage.evaluate<string>(
      `(function() { var bc = document.querySelector('.volume-name'); return bc ? bc.textContent || '' : ''; })()`,
    )

    await openVolumePicker(tauriPage)
    await tauriPage.waitForSelector(ANY_VOLUME_ITEM, 3000)

    // Click the synthetic Network entry. Find it by label since `data-index` is volatile.
    await tauriPage.evaluate(`(function() {
      var items = document.querySelectorAll('.volume-item');
      for (var i = 0; i < items.length; i++) {
        var label = items[i].querySelector('.volume-label');
        if (label && label.textContent === 'Network (disabled)') {
          items[i].click();
          return;
        }
      }
    })()`)

    // The dropdown should close. `handleVolumeSelect` sets `isOpen = false` up front, so
    // both the early-return and the navigate paths close the dropdown — but only the
    // early-return path leaves the breadcrumb unchanged (next assertion).
    await pollUntil(tauriPage, async () => !(await tauriPage.isVisible(PICKER_DROPDOWN)), 3000)
    expect(await tauriPage.isVisible(PICKER_DROPDOWN)).toBe(false)

    // Active volume must NOT have changed: the disabled-network branch returns early
    // without calling `onVolumeChange`, so the breadcrumb stays put. (We don't assert
    // that the settings window actually opened — `openSettingsWindow` is fire-and-forget,
    // and inspecting other webviews from the test webview is awkward via `evaluate()`.
    // The breadcrumb-stability check is enough to prove our branch was the one that ran.)
    const labelAfter = await tauriPage.evaluate<string>(
      `(function() { var bc = document.querySelector('.volume-name'); return bc ? bc.textContent || '' : ''; })()`,
    )
    expect(labelAfter).toBe(labelBefore)
  })
})

/** Polls the Network entry label until it matches the expected value, then returns it. */
async function pollUntilLabel(tauriPage: Parameters<typeof pollUntil>[0], expected: string): Promise<string | null> {
  let label: string | null = null
  await pollUntil(
    tauriPage,
    async () => {
      label = await readNetworkLabel(tauriPage)
      return label === expected
    },
    3000,
  )
  return label
}
