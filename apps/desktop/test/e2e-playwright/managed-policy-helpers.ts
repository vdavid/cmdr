/**
 * Shared plumbing for the managed-policy (MDM) specs. The `playwright-e2e` build reads the policy
 * from the plist `CMDR_MANAGED_PREFS_FILE` names and watches it, so a spec writes a policy, waits
 * for the backend to see it, and removes it again so every later spec in the shard runs unmanaged.
 */

import fs from 'fs'
import { waitBudget } from './wait-budget.js'
import { expect } from './fixtures.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/** Writes a plist whose top-level dict holds `dictBody`, in one rename (the watcher never reads half a file). */
export function writePolicyFile(file: string, dictBody: string): void {
  const contents = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
${dictBody}
</dict>
</plist>
`
  const temp = `${file}.writing`
  fs.writeFileSync(temp, contents)
  fs.renameSync(temp, file)
}

/** Whether the app's backend currently reads a managed policy. */
export async function appSeesManagedPolicy(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(
    `(async function(){ return (await window.__TAURI_INTERNALS__.invoke('get_managed_policy')).managed; })()`,
  )
}

/** Waits until the backend reads the policy just written. */
export async function waitForManagedPolicy(page: TauriPage): Promise<void> {
  await expect.poll(() => appSeesManagedPolicy(page), { timeout: waitBudget(5000) }).toBe(true)
}

/** Removes the policy file and waits until the app runs unmanaged again. */
export async function removePolicyFileAndWait(page: TauriPage, file: string): Promise<void> {
  fs.rmSync(file, { force: true })
  await expect.poll(() => appSeesManagedPolicy(page), { timeout: waitBudget(5000) }).toBe(false)
}

/** JS that clicks a settings sidebar `.section-item` by exact (trimmed) text; false when there's none. */
export function clickSettingsSectionJs(name: string): string {
  return `(function() {
    var items = document.querySelectorAll('.section-item');
    for (var i = 0; i < items.length; i++) {
      if ((items[i].textContent || '').trim() === ${JSON.stringify(name)}) { items[i].click(); return true; }
    }
    return false;
  })()`
}
