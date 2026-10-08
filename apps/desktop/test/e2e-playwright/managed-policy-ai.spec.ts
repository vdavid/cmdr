/**
 * E2E for the organization's AI policy (MDM) as Settings › AI › Provider shows it.
 *
 * The app reads the policy from the plist `CMDR_MANAGED_PREFS_FILE` names (the check runner points
 * every shard at a file that doesn't exist: no policy). Each test writes one, waits for the backend
 * to see it, checks what the provider controls show, and removes it in `afterEach`.
 *
 * What it guards end to end: the `ai.provider` lock reaching the bespoke provider radios with its
 * visible reason (`DisableCloudAI`), and the backend's per-URL host verdicts reaching the service
 * dropdown (`AllowedCloudAIHosts`), where a refused service stays listed but disabled.
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
const PROVIDER_SECTION = '[data-section-id="ai-provider"]'

/** JS that clicks the provider radio labeled `label`; false when there's none. */
function clickProviderOptionJs(label: string): string {
  return `(function() {
    var radios = document.querySelectorAll('${PROVIDER_SECTION} [role="radio"]');
    for (var i = 0; i < radios.length; i++) {
      if (radios[i].textContent.trim() === ${JSON.stringify(label)}) { radios[i].click(); return true; }
    }
    return false;
  })()`
}

/** JS reading each provider radio's disabled state, by label. */
const PROVIDER_RADIOS_JS = `(function() {
  var out = {};
  document.querySelectorAll('${PROVIDER_SECTION} [role="radio"]').forEach(function(r) {
    out[r.textContent.trim()] = r.disabled;
  });
  return out;
})()`

/** JS reading whether each service is disabled in the dropdown (Ark's hidden native select mirrors it). */
const SERVICE_OPTIONS_JS = `(function() {
  var out = {};
  document.querySelectorAll('${PROVIDER_SECTION} select option').forEach(function(o) {
    if (o.value) out[o.value] = o.disabled;
  });
  return out;
})()`

async function openProviderSection(main: TauriPage): Promise<TauriPage> {
  const settings = await openSettingsWindowViaProd(main)
  await settings.waitForSelector('.settings-sidebar', waitBudget(3000))
  expect(await settings.evaluate<boolean>(clickSettingsSectionJs('Provider'))).toBe(true)
  await settings.waitForSelector(`${PROVIDER_SECTION} [role="radiogroup"]`, waitBudget(5000))
  return settings
}

test.describe('Managed AI policy (MDM)', () => {
  // The macOS check runner sets it for every shard; without it there's no file the app watches.
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

  test('on-device only rules out Cloud AI in Settings, with the reason in view', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    if (!policyFile) return
    writePolicyFile(policyFile, '  <key>DisableCloudAI</key><true/>')
    await waitForManagedPolicy(main)

    settings = await openProviderSection(main)
    const radios = await settings.evaluate<Record<string, boolean>>(PROVIDER_RADIOS_JS)
    // Local LLM's state is the Mac's (Apple Silicon only), not the policy's.
    expect(radios['Off']).toBe(false)
    expect(radios['Cloud AI']).toBe(true)
    await settings.waitForSelector('#settings-ai-provider-managed', waitBudget(3000))
    const note = await settings.evaluate<string>(
      `(document.getElementById('settings-ai-provider-managed') || {}).textContent.trim()`,
    )
    expect([
      'Your organization allows only on-device AI.',
      'Your organization allows only on-device AI, and this Mac can’t run it.',
    ]).toContain(note)
  })

  test('a host list keeps refused services listed but disabled in the service dropdown', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    if (!policyFile) return
    writePolicyFile(policyFile, '  <key>AllowedCloudAIHosts</key><array><string>api.anthropic.com</string></array>')
    await waitForManagedPolicy(main)

    settings = await openProviderSection(main)
    const page = settings
    try {
      expect(await page.evaluate<boolean>(clickProviderOptionJs('Cloud AI'))).toBe(true)
      // Poll the DOM, ❌ never `waitForSelector`: that waits for VISIBLE, and Ark's hidden select
      // (sr-only, inside the `inert` setup card while Allow cloud AI is off) never is.
      await expect
        .poll(async () => (await page.evaluate<Record<string, boolean>>(SERVICE_OPTIONS_JS))['openai'], {
          timeout: waitBudget(5000),
        })
        .toBe(true)
      const options = await page.evaluate<Record<string, boolean>>(SERVICE_OPTIONS_JS)
      expect(options['anthropic']).toBe(false)
      // An editable endpoint is judged once entered, never by its placeholder.
      expect(options['custom']).toBe(false)
    } finally {
      // Put the AI mode back, so no later spec inherits Cloud.
      await page.evaluate<boolean>(clickProviderOptionJs('Off'))
    }
  })
})
