/**
 * E2E for the image-index importance slider (`mediaIndex.importanceThreshold`).
 *
 * The required slider E2E: the slider persists a new level and its live preview updates as the
 * user moves it. The slider only shows once the master "Index image contents" toggle is on AND
 * the scope is the automatic one, so the spec sets both first (and turns indexing back off at
 * the end so no state leaks). The default scope indexes only the folders the user chose, where
 * the threshold has no effect and the slider is deliberately absent.
 *
 * The slider renders named buckets over a typed `0.0..=1.0` threshold; the rightmost bucket
 * (the default) is threshold `0.0` ("everywhere"). Pressing ArrowLeft on the thumb moves one
 * bucket toward "most-used only" (threshold `0.2`), which the sparse store flushes and the
 * primary bucket label reflects live. Persistence is confirmed against the on-disk
 * `settings.json` (the store debounces ~500 ms, so polling the file is deterministic).
 *
 * Follows the settings-window harness in `image-index-settings.spec.ts`.
 */

import fs from 'node:fs'
import path from 'node:path'
import { test, expect } from './fixtures.js'
import { closeScopedWindow, openSettingsWindowViaProd } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const MASTER_LABEL = 'Index image contents'
const SECTION_ID = 'ai-image-search'
const MASTER_KEY = 'mediaIndex.enabled'
const THRESHOLD_KEY = 'mediaIndex.importanceThreshold'
const AUTOMATIC_SCOPE = 'importance'

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

function clickSectionByTextJs(name: string): string {
  return `(function() {
    var items = document.querySelectorAll('.section-item');
    var target = ${JSON.stringify(name)};
    for (var i = 0; i < items.length; i++) {
      if ((items[i].textContent || '').trim() === target) { items[i].click(); return true; }
    }
    return false;
  })()`
}

/** Clicks the master toggle control; returns false if absent. */
function clickMasterJs(): string {
  return `(function() {
    var root = document.querySelector('[aria-label=${JSON.stringify(MASTER_LABEL)}]');
    if (!root) return false;
    var control = root.querySelector('.switch-control') || root;
    control.click();
    return true;
  })()`
}

/** Clicks the automatic-scope radio option; returns false if absent. */
function clickAutomaticScopeJs(): string {
  return `(function() {
    var input = document.querySelector('input[value=${JSON.stringify(AUTOMATIC_SCOPE)}]');
    if (!input) return false;
    input.click();
    return true;
  })()`
}

/** The slider's current primary bucket label text ('' if the slider isn't shown). */
function bucketLabelJs(): string {
  return `(function() {
    var el = document.querySelector('.mi-slider-value');
    return el ? (el.textContent || '').trim() : '';
  })()`
}

/** Focuses the slider thumb and dispatches an arrow key Ark UI's slider listens for. */
function pressArrowJs(key: 'ArrowLeft' | 'ArrowRight'): string {
  return `(function() {
    var thumb = document.querySelector('.mi-slider-thumb');
    if (!thumb) return false;
    thumb.focus();
    thumb.dispatchEvent(new KeyboardEvent('keydown', { key: ${JSON.stringify(key)}, bubbles: true, cancelable: true }));
    return true;
  })()`
}

async function openImageSearch(settings: TauriPage): Promise<void> {
  const clicked = await settings.evaluate<boolean>(clickSectionByTextJs('Image search'))
  expect(clicked, 'Image search sidebar item exists').toBe(true)
  await settings.waitForSelector(`[data-section-id="${SECTION_ID}"]`, 3000)
  await settings.waitForSelector(`[aria-label="${MASTER_LABEL}"]`, 3000)
}

async function openSettings(tauriPage: TauriPage): Promise<TauriPage> {
  const settings = await openSettingsWindowViaProd(tauriPage)
  await settings.waitForSelector('.settings-window', 3000)
  await settings.waitForSelector('.settings-sidebar', 3000)
  await openImageSearch(settings)
  return settings
}

test.describe('Image-index importance slider', () => {
  test('persists a new level and updates its preview label', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    const settings = await openSettings(main)

    // Enable image indexing so the scope control reveals.
    expect(await settings.evaluate<boolean>(clickMasterJs())).toBe(true)
    await expect.poll(() => settingOnDisk(MASTER_KEY), { timeout: 3000 }).toBe(true)
    await settings.waitForSelector('.mi-scope', 3000)

    // The default scope is "only folders I choose", where the threshold does nothing — so the
    // slider isn't there at all. Pick the automatic scope to reveal it.
    expect(await settings.evaluate<string>(bucketLabelJs()), 'no slider in the default scope').toBe('')
    expect(await settings.evaluate<boolean>(clickAutomaticScopeJs())).toBe(true)
    // The slider appearing IS the scope taking effect, so wait on that rather than on the
    // debounced settings write — this spec's subject is the threshold's persistence, and
    // `mediaIndex.scope`'s has its own coverage in `MediaIndexScope.a11y.test.ts`.
    await settings.waitForSelector('.mi-slider-thumb', 3000)

    // Default position is the broadest bucket ("everywhere"), threshold 0.0 (sparse ⇒ unset).
    const initialLabel = await settings.evaluate<string>(bucketLabelJs())
    expect(initialLabel.length, 'slider shows a named bucket label').toBeGreaterThan(0)

    // Move one bucket toward "most-used only": the typed threshold flushes to 0.2 and the
    // primary label changes live.
    expect(await settings.evaluate<boolean>(pressArrowJs('ArrowLeft'))).toBe(true)
    await expect.poll(() => settingOnDisk(THRESHOLD_KEY), { timeout: 3000 }).toBe(0.2)
    await expect.poll(async () => settings.evaluate<string>(bucketLabelJs()), { timeout: 3000 }).not.toBe(initialLabel)

    // Move back to the broadest bucket, then turn indexing off so no state leaks into later specs.
    expect(await settings.evaluate<boolean>(pressArrowJs('ArrowRight'))).toBe(true)
    await expect.poll(() => settingOnDisk(THRESHOLD_KEY), { timeout: 3000 }).toBe(0)
    expect(await settings.evaluate<boolean>(clickMasterJs())).toBe(true)
    await expect.poll(() => settingOnDisk(MASTER_KEY), { timeout: 3000 }).toBe(false)

    await closeScopedWindow(main, settings, 'settings')
  })
})
