/**
 * E2E for the "Index image contents" master toggle (`mediaIndex.enabled`).
 *
 * The OCR slice's one required E2E: the toggle persists across a settings-window reload. The
 * setting is off by default; turning it on writes through the sparse settings store (in
 * the isolated `CMDR_DATA_DIR`) and live-applies to the backend `media_index` scheduler
 * via `set_image_index_enabled`. Reopening the window must re-read the persisted `true`.
 *
 * Persistence is confirmed against the on-disk `settings.json` (polling the file is
 * deterministic where closing-then-reopening would race the flush), then the reopened
 * window is checked to prove it re-reads that value. The store debounces its save
 * ~500 ms, so each disk assertion is preceded by the E2E flush hook rather than waiting
 * the window out.
 *
 * Follows the settings-window harness in `settings.spec.ts`: open via the production
 * `open-settings` trigger, scope a `TauriPage`, drive the real Ark Switch by its label.
 */

import fs from 'node:fs'
import path from 'node:path'
import { test, expect } from './fixtures.js'
import { closeScopedWindow, openSettingsWindowViaProd } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const SWITCH_LABEL = 'Index image contents'
const SECTION_ID = 'indexing-image-indexing'
const SETTING_KEY = 'mediaIndex.enabled'

const settingsFilePath = (() => {
  const dataDir = process.env.CMDR_DATA_DIR
  if (!dataDir) throw new Error('CMDR_DATA_DIR env var is not set; this spec needs an isolated app instance')
  return path.join(dataDir, 'settings.json')
})()

/** Reads one key from the instance's settings.json; `undefined` when file/key absent. */
function settingOnDisk(key: string): unknown {
  try {
    const parsed: unknown = JSON.parse(fs.readFileSync(settingsFilePath, 'utf-8'))
    if (typeof parsed !== 'object' || parsed === null) return undefined
    return (parsed as Record<string, unknown>)[key]
  } catch {
    return undefined
  }
}

/** Clicks a sidebar `.section-item` by exact trimmed text; returns false if not found. */
function clickSectionByTextJs(name: string): string {
  return `(function() {
    var items = document.querySelectorAll('.section-item');
    var target = ${JSON.stringify(name)};
    for (var i = 0; i < items.length; i++) {
      if ((items[i].textContent || '').trim() === target) {
        items[i].click();
        return true;
      }
    }
    return false;
  })()`
}

/**
 * `[aria-label="…"]` resolves to the switch's hidden `<input>`, not to the Ark root: that's the
 * element assistive tech names (see `lib/ui/DETAILS.md` § Switch). So state reads have to climb
 * to the root first, since `data-state` lives on the styled track.
 *
 * Clicks go on the INPUT, never on the track: the track sits inside the `<label>`, so clicking
 * it also triggers label activation and the toggle can fire twice.
 */
function switchStateJs(): string {
  return `(function() {
    var input = document.querySelector('[aria-label=${JSON.stringify(SWITCH_LABEL)}]');
    if (!input) return 'missing';
    var root = input.closest('[data-scope="switch"][data-part="root"]');
    var control = root ? root.querySelector('.switch-control') : null;
    return (control || input).getAttribute('data-state') || 'unknown';
  })()`
}

/**
 * Forces the settings store's pending write to disk, so assertions on `settings.json`
 * don't have to wait out the 500 ms save debounce. The hook is installed by the settings
 * page under `CMDR_E2E_MODE` and runs the same `forceSave` the window uses on close.
 */
function flushSettingsJs(): string {
  return `(function() {
    var flush = window.__cmdrFlushSettings;
    if (typeof flush !== 'function') return false;
    flush();
    return true;
  })()`
}

/** Clicks the toggle; returns false if the switch isn't present. */
function clickSwitchJs(): string {
  return `(function() {
    var input = document.querySelector('[aria-label=${JSON.stringify(SWITCH_LABEL)}]');
    if (!input) return false;
    input.click();
    return true;
  })()`
}

/** Navigates the settings window to Indexing > Image indexing and waits for its wrapper + switch. */
async function openImageIndexing(settings: TauriPage): Promise<void> {
  const clicked = await settings.evaluate<boolean>(clickSectionByTextJs('Image indexing'))
  expect(clicked, 'Image indexing sidebar item exists').toBe(true)
  await settings.waitForSelector(`[data-section-id="${SECTION_ID}"]`, 3000)
  await settings.waitForSelector(`[aria-label="${SWITCH_LABEL}"]`, 3000)
}

async function openSettings(tauriPage: TauriPage): Promise<TauriPage> {
  const settings = await openSettingsWindowViaProd(tauriPage)
  await settings.waitForSelector('.settings-window', 3000)
  await settings.waitForSelector('.settings-sidebar', 3000)
  await openImageIndexing(settings)
  return settings
}

test.describe('Image-content indexing toggle', () => {
  test('persists across a settings-window reload', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage

    // First open: the toggle exists and defaults off (no persisted key).
    let settings = await openSettings(main)
    expect(await settings.evaluate<string>(switchStateJs()), 'defaults off').toBe('unchecked')

    // Turn it on; the switch reflects it in-window, then the sparse store flushes `true`.
    expect(await settings.evaluate<boolean>(clickSwitchJs())).toBe(true)
    await expect.poll(async () => settings.evaluate<string>(switchStateJs()), { timeout: 3000 }).toBe('checked')
    expect(await settings.evaluate<boolean>(flushSettingsJs()), 'E2E flush hook installed').toBe(true)
    await expect.poll(() => settingOnDisk(SETTING_KEY), { timeout: 3000 }).toBe(true)

    // Reopen: a fresh window must re-read the persisted `true`.
    await closeScopedWindow(main, settings, 'settings')
    settings = await openSettings(main)
    await expect.poll(async () => settings.evaluate<string>(switchStateJs()), { timeout: 3000 }).toBe('checked')

    // Reset to the default off so the persisted state doesn't leak into other specs.
    expect(await settings.evaluate<boolean>(clickSwitchJs())).toBe(true)
    await expect.poll(async () => settings.evaluate<string>(switchStateJs()), { timeout: 3000 }).toBe('unchecked')
    await settings.evaluate<boolean>(flushSettingsJs())
    await expect.poll(() => settingOnDisk(SETTING_KEY), { timeout: 3000 }).toBe(false)

    await closeScopedWindow(main, settings, 'settings')
  })
})
