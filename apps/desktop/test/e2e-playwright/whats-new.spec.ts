/**
 * E2E for the automatic "What's new" post-update popup and its opt-out.
 *
 * The boot auto-check is suppressed under E2E mode (the FDA mock makes the app
 * boot onboarded, so the inaugural-showcase popup would otherwise leak into
 * whichever spec runs first), keeping every other spec popup-free. To exercise
 * the real auto-show path this spec emits the E2E-gated `e2e-rerun-whats-new`
 * event, which seeds `isOnboarded` + an old `lastSeenVersion` and force-runs the
 * SAME `maybeRunWhatsNew()` the boot path uses. (The seed is non-emitting on
 * purpose; see the handler in `routes/(main)/+page.svelte` and
 * `lib/whats-new/CLAUDE.md` § "E2E seam".)
 *
 * Flow: seed (onboarded, lastSeen `0.1.0`, enabled) → assert the popup shows
 * 1–5 version blocks → click "Not interested in changelogs" → assert it closes,
 * `whatsNew.showOnUpdate` flipped to false on disk, and `whatsNew.lastSeenVersion`
 * stamped to the running version (so a relaunch shows nothing). The opt-out
 * leaves the feature off, so the shared instance ends the test popup-free.
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { expectAndDismissToast } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const settingsFilePath = (() => {
  const dataDir = process.env.CMDR_DATA_DIR
  if (!dataDir) throw new Error('CMDR_DATA_DIR env var is not set; this spec needs an isolated app instance')
  return path.join(dataDir, 'settings.json')
})()

/** Reads one key from the instance's settings.json. `undefined` when the file or
 *  key is absent (the store deletes keys equal to their default). */
function settingOnDisk(key: string): unknown {
  try {
    const parsed: unknown = JSON.parse(fs.readFileSync(settingsFilePath, 'utf-8'))
    if (typeof parsed !== 'object' || parsed === null) return undefined
    return (parsed as Record<string, unknown>)[key]
  } catch {
    return undefined
  }
}

/** The version the running app reports (same source the trigger stamps from). */
function runningVersion(page: TauriPage): Promise<string> {
  return page.evaluate<string>(`window.__TAURI_INTERNALS__.invoke('plugin:app|version')`)
}

/** Number of release blocks rendered in the open popup. */
function releaseBlockCount(page: TauriPage): Promise<number> {
  return page.evaluate<number>(`document.querySelectorAll('#whats-new-body .release').length`)
}

function popupOpen(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(`document.querySelector('#whats-new-body') !== null`)
}

/** Clicks the footer "Not interested in changelogs" opt-out link by its text. */
function clickOptOut(page: TauriPage): Promise<void> {
  return page.evaluate(`
    (function () {
      const links = document.querySelectorAll('#whats-new-body .footer .link-button')
      for (const link of links) {
        if ((link.textContent || '').trim() === 'Not interested in changelogs') {
          link.click()
          return
        }
      }
      throw new Error('opt-out link not found')
    })()
  `)
}

/** Removes the keys this spec seeds so the shared settings.json ends popup-free
 *  (matters for repeated local runs against an isolated instance). */
function restoreSettingsOnDisk(): void {
  try {
    const parsed: unknown = JSON.parse(fs.readFileSync(settingsFilePath, 'utf-8'))
    if (typeof parsed !== 'object' || parsed === null) return
    const obj = parsed as Record<string, unknown>
    delete obj['whatsNew.lastSeenVersion']
    delete obj['whatsNew.showOnUpdate']
    delete obj['isOnboarded']
    fs.writeFileSync(settingsFilePath, JSON.stringify(obj))
  } catch {
    // File absent or unreadable: nothing to restore.
  }
}

test.describe("What's new popup", () => {
  test.describe.configure({ timeout: 30000 })

  test.afterEach(() => {
    restoreSettingsOnDisk()
  })

  test('auto-shows on update, opts out, and stamps the running version', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const version = await runningVersion(page)

    // Seed (onboarded, far-behind lastSeen, feature on) and re-run the real
    // auto-trigger. `0.1.0` predates every release, so the slice is non-empty
    // and capped at five.
    await page.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'e2e-rerun-whats-new',
      payload: { isOnboarded: true, lastSeenVersion: '0.1.0', showOnUpdate: true }
    })`)

    // The popup auto-appears with at least one and at most five version blocks.
    await expect.poll(() => popupOpen(page), { timeout: 5000 }).toBe(true)
    const blocks = await releaseBlockCount(page)
    expect(blocks).toBeGreaterThanOrEqual(1)
    expect(blocks).toBeLessThanOrEqual(5)

    // The auto-show path stamps the running version even before the user acts.
    await expect.poll(() => settingOnDisk('whatsNew.lastSeenVersion'), { timeout: 5000 }).toBe(version)

    // Opt out: the dialog closes, the feature flips off, and a toast confirms it.
    await clickOptOut(page)
    await expect.poll(() => popupOpen(page), { timeout: 5000 }).toBe(false)
    await expectAndDismissToast(page, 'no more update notes')
    await expect.poll(() => settingOnDisk('whatsNew.showOnUpdate'), { timeout: 5000 }).toBe(false)

    // Relaunch would show nothing: lastSeen now equals the running version, so
    // the version-unchanged rule fires (and the feature is off anyway).
    expect(settingOnDisk('whatsNew.lastSeenVersion')).toBe(version)
  })
})
