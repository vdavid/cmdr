/**
 * E2E for the organization's managed policy (MDM) as Settings › Updates & privacy shows it.
 *
 * The app reads the policy from the plist `CMDR_MANAGED_PREFS_FILE` names (the `playwright-e2e`
 * build honors it in place of CFPreferences), and the check runner points every shard at a file
 * in its own data dir that doesn't exist: no policy. This spec writes one, waits for the app to
 * re-read it (it watches the file), checks the locked rows and their reasons, and removes it
 * again in `afterEach`, so every other spec runs unmanaged.
 *
 * What it guards end to end: the backend's `locked_settings` reaching the settings window, every
 * row primitive picking up the lock, the section line, the disabled check button with its reason,
 * and the "Managed by your organization" summary.
 */

import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import { closeScopedWindow, openSettingsWindowViaProd } from './helpers.js'
import {
  clickSettingsSectionJs,
  removePolicyFileAndWait,
  waitForManagedPolicy,
  writePolicyFile,
} from './managed-policy-helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const policyFile = process.env.CMDR_MANAGED_PREFS_FILE

const POLICY = `  <key>DisableUsageStats</key><true/>
  <key>DisableCrashAndErrorReports</key><true/>
  <key>DisableUpdates</key><true/>`

const ROW_NOTE = 'Your organization manages this setting.'
const LOCKED_IDS = ['analytics.enabled', 'updates.crashReports', 'updates.errorReports', 'updates.autoCheck']

interface UpdatesProbe {
  sectionNote: boolean
  rows: { id: string; switchDisabled: boolean | null; note: string }[]
  checkButtonDisabled: boolean | null
  status: string
  summary: string[]
}

function probeUpdatesSectionJs(ids: string[]): string {
  return `(function() {
    var section = document.querySelector('[data-section-id="updates"]');
    if (!section) return null;
    var rows = ${JSON.stringify(ids)}.map(function(id) {
      var row = document.getElementById('setting-' + id);
      var control = row ? row.querySelector('input[role="switch"]') : null;
      var note = document.getElementById('setting-' + id + '-disabled-note');
      return { id: id, switchDisabled: control ? control.disabled : null, note: note ? note.textContent.trim() : '' };
    });
    var check = Array.from(section.querySelectorAll('button')).find(function(b) {
      return b.textContent.trim() === 'Check for updates';
    });
    var status = section.querySelector('.status-text');
    var summary = Array.from(section.querySelectorAll('dt')).map(function(dt) {
      return dt.textContent.trim() + ': ' + (dt.nextElementSibling ? dt.nextElementSibling.textContent.trim() : '');
    });
    return {
      sectionNote: !!section.querySelector('.managed-section-note'),
      rows: rows,
      checkButtonDisabled: check ? check.disabled : null,
      status: status ? status.textContent.trim() : '',
      summary: summary,
    };
  })()`
}

test.describe('Managed policy (MDM)', () => {
  // The macOS check runner sets it for every shard; a hand-run suite or a lane that doesn't has
  // no file the app is watching, so there's nothing to push a policy through.
  test.skip(!policyFile, 'CMDR_MANAGED_PREFS_FILE is not set for this run')

  let settings: TauriPage | undefined

  test.afterEach(async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    try {
      if (settings) await closeScopedWindow(main, settings, 'settings')
    } finally {
      settings = undefined
      // Every later spec in this shard must run unmanaged.
      if (policyFile) await removePolicyFileAndWait(main, policyFile)
    }
  })

  test('Settings › Updates & privacy shows each managed row locked, with its reason and a summary', async ({
    tauriPage,
  }) => {
    const main = tauriPage as TauriPage
    if (!policyFile) return
    writePolicyFile(policyFile, POLICY)
    await waitForManagedPolicy(main)

    settings = await openSettingsWindowViaProd(main)
    await settings.waitForSelector('.settings-sidebar', waitBudget(3000))
    expect(await settings.evaluate<boolean>(clickSettingsSectionJs('Updates & privacy'))).toBe(true)
    await settings.waitForSelector('[data-section-id="updates"] .managed-section-note', waitBudget(3000))

    const probe = await settings.evaluate<UpdatesProbe | null>(probeUpdatesSectionJs(LOCKED_IDS))
    expect(probe).not.toBeNull()
    if (!probe) return
    expect(probe.sectionNote).toBe(true)
    for (const row of probe.rows) {
      expect(row, row.id).toEqual({ id: row.id, switchDisabled: true, note: ROW_NOTE })
    }
    expect(probe.checkButtonDisabled).toBe(true)
    expect(probe.status).toBe('Your organization manages updates for Cmdr.')
    expect(probe.summary).toEqual(['Usage stats: Off', 'Crash and error reports: Off', 'Updates: Off'])
  })
})
