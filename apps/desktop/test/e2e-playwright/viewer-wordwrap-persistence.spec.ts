/**
 * Cross-session E2E for viewer word-wrap persistence.
 *
 * The viewer window deliberately has NO `tauri-plugin-store` capability (it
 * renders arbitrary, possibly-hostile file content; see
 * `src-tauri/capabilities/CLAUDE.md` § viewer). Persistence therefore flows
 * through typed backend commands: `get_restricted_window_settings` (read at
 * open) and `persist_restricted_window_setting` (write via the main window).
 * Only an E2E exercises that full path under the real capability ACL — a
 * vitest mock can't reproduce the permission denial that broke this once
 * (every viewer open fired an error-level "store load not allowed by ACL"
 * log and word wrap silently reset to default each session).
 *
 * Flow: open viewer → toggle wrap with `W` → wait for the toggle to land in
 * settings.json → reopen → assert wrap is still on → toggle off → reopen →
 * assert off (which doubles as cleanup, so the shared settings file ends the
 * test in its default state).
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { closeScopedWindow, openViewerWindow } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const fixtureRoot = (() => {
  const root = process.env.CMDR_E2E_START_PATH
  if (!root)
    throw new Error('CMDR_E2E_START_PATH env var is not set; fixtures must be created before running this spec')
  return root
})()
const testFilePath = path.join(fixtureRoot, 'left', 'file-a.txt')

// The persistence assertion reads the instance's settings.json directly: the
// FE save pipeline is debounced (500 ms), so "the toggle landed on disk" is
// the only deterministic signal that a reopened viewer will read the new
// value. Requires an isolated instance; refusing to run against the real
// production settings file is deliberate (same reasoning as the fixtureRoot
// throw above: a silent fallback would hide setup bugs AND scribble on the
// developer's real config).
const settingsFilePath = (() => {
  const dataDir = process.env.CMDR_DATA_DIR
  if (!dataDir) throw new Error('CMDR_DATA_DIR env var is not set; this spec needs an isolated app instance')
  return path.join(dataDir, 'settings.json')
})()

/** Reads `viewer.wordWrap` from the instance's settings.json. `undefined` when
 *  the file or key is absent (the store deletes keys equal to the default). */
function wordWrapOnDisk(): boolean | undefined {
  try {
    const parsed: unknown = JSON.parse(fs.readFileSync(settingsFilePath, 'utf-8'))
    if (typeof parsed !== 'object' || parsed === null) return undefined
    const value: unknown = (parsed as Record<string, unknown>)['viewer.wordWrap']
    return typeof value === 'boolean' ? value : undefined
  } catch {
    // File absent or mid-write: treat as "not persisted yet" and keep polling.
    return undefined
  }
}

async function openViewerForFile(mainPage: TauriPage, filePath: string): Promise<TauriPage> {
  const viewer = await openViewerWindow(mainPage, filePath)
  await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', 10000)
  return viewer
}

/** True when the status bar shows the "wrap" badge (the word-wrap indicator). */
function wrapBadgeVisible(viewer: TauriPage): Promise<boolean> {
  return viewer.evaluate<boolean>(`
    (function () {
      const badges = document.querySelectorAll('.status-bar .backend-badge')
      for (const badge of badges) {
        if ((badge.textContent || '').trim() === 'wrap') return true
      }
      return false
    })()
  `)
}

/** Dispatches an unmodified `w` keydown on the viewer window (the production
 *  word-wrap toggle binding, handled by the `<svelte:window>` listener). */
async function pressWrapToggle(viewer: TauriPage): Promise<void> {
  await viewer.evaluate(`window.dispatchEvent(new KeyboardEvent('keydown', { key: 'w', bubbles: true }))`)
}

test.describe('Viewer word-wrap persistence', () => {
  // Three sequential viewer sessions plus two 500 ms-debounced disk writes.
  // `retries: 1` covers a transient open/debounce hiccup. The real hazard is a
  // PRIOR run that died before its Session-3 cleanup, leaving `viewer.wordWrap`
  // on in the isolated settings.json — then Session 1's "default off" assertion
  // fails (and a retry alone can't recover persisted state). The afterEach below
  // always resets the key so a failed run can't poison the next.
  test.describe.configure({ timeout: 60000, retries: 1 })

  // Self-heal: clear the persisted toggle after each attempt (even on failure),
  // so the next run/retry starts from the default-off baseline this spec asserts.
  test.afterEach(() => {
    try {
      const raw = fs.readFileSync(settingsFilePath, 'utf-8')
      const parsed: unknown = JSON.parse(raw)
      if (typeof parsed === 'object' && parsed !== null && 'viewer.wordWrap' in parsed) {
        delete (parsed as Record<string, unknown>)['viewer.wordWrap']
        fs.writeFileSync(settingsFilePath, JSON.stringify(parsed, null, 2))
      }
    } catch {
      // No settings file yet, or mid-write: nothing to reset.
    }
  })

  test('word wrap toggled in one viewer session is on in the next', async ({ tauriPage }) => {
    const mainPage = tauriPage as TauriPage

    // Session 1: default off, toggle on, wait for the write to land on disk.
    const viewer1 = await openViewerForFile(mainPage, testFilePath)
    const label1 = viewer1.targetWindow
    if (!label1) throw new Error('Scoped viewer page has no targetWindow label')
    expect(await wrapBadgeVisible(viewer1)).toBe(false)

    await pressWrapToggle(viewer1)
    await expect.poll(() => wrapBadgeVisible(viewer1), { timeout: 3000 }).toBe(true)
    await expect.poll(() => wordWrapOnDisk(), { timeout: 5000 }).toBe(true)
    await closeScopedWindow(mainPage, viewer1, label1)

    // Session 2: the regression assertion — wrap must come back on.
    const viewer2 = await openViewerForFile(mainPage, testFilePath)
    const label2 = viewer2.targetWindow
    if (!label2) throw new Error('Scoped viewer page has no targetWindow label')
    expect(await wrapBadgeVisible(viewer2)).toBe(true)

    // Toggle back off and wait for the key to drop from disk (false is the
    // default, so the store removes it). This is the cleanup: the shared
    // settings file ends in its pre-test state.
    await pressWrapToggle(viewer2)
    await expect.poll(() => wrapBadgeVisible(viewer2), { timeout: 3000 }).toBe(false)
    await expect.poll(() => wordWrapOnDisk(), { timeout: 5000 }).not.toBe(true)
    await closeScopedWindow(mainPage, viewer2, label2)

    // Session 3: verify the off state round-trips too.
    const viewer3 = await openViewerForFile(mainPage, testFilePath)
    const label3 = viewer3.targetWindow
    if (!label3) throw new Error('Scoped viewer page has no targetWindow label')
    expect(await wrapBadgeVisible(viewer3)).toBe(false)
    await closeScopedWindow(mainPage, viewer3, label3)
  })
})
