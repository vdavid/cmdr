/**
 * Tells an app instance it's running on an English, US-formatting machine,
 * before the binary launches.
 *
 * Cmdr reads BOTH locale answers from the OS: the UI language from the ordered
 * language preferences, and the number/date conventions from the region
 * (`src-tauri/src/intl/`). That's right for a user and wrong for a harness. On a
 * Hungarian Mac every asserted string comes out Hungarian, and on a
 * Swedish-region Mac (a common shape: US English, Swedish region) a copy dialog
 * renders `1,00 KB` where the spec asserts `1.00 KB`. Both read as product
 * regressions and neither is one.
 *
 * The two answers need two pins, because they come from different places:
 *
 * - {@link pinUiLanguage} writes the `appearance.language` SETTING, which is
 *   what a user picking English in Settings > Appearance writes, so the app
 *   under test runs production code all the way down. Works on every platform.
 * - {@link EN_US_LOCALE_ARGS} are macOS process arguments. Formatting follows
 *   the OS region and no setting overrides it, deliberately (`src/lib/intl/`),
 *   so the only honest lever is to hand the process a different OS answer.
 *
 * ❌ Neither is the pseudolocale path. The overflow pass drives
 * `setLocale('en-XA')` against the RUNNING app and touches neither of these.
 */

import fs from 'fs'
import path from 'path'

/** The setting every harness pins, and what it pins it to. */
const LANGUAGE_KEY = 'appearance.language'
const ENGLISH = 'en'

/**
 * Process arguments that make a macOS launch look like an en-US machine.
 *
 * `NSUserDefaults` reads its argument domain first, so these outrank the
 * machine's own System Settings for this process alone: nothing global changes,
 * and a developer's Mac stays exactly as they left it. `AppleLocale` is the half
 * that matters most, since it carries the region override Foundation formats by
 * (`en_US@rg=sezzzz` on a US-English Mac living in Sweden); `AppleLanguages`
 * makes the native menu bar English from the FIRST frame, before the frontend
 * pushes the pinned setting down.
 *
 * Inert on Linux, which has no `NSUserDefaults`, so passing them there would
 * only be noise.
 */
export const EN_US_LOCALE_ARGS: readonly string[] = ['-AppleLocale', 'en_US', '-AppleLanguages', '(en-US)']

/**
 * Merges the English pin into `<dataDir>/settings.json`, creating the file (and
 * the directory) when they aren't there yet.
 *
 * Merges rather than overwrites: the marketing-shots instance keeps a data dir
 * between runs, with pane paths, tabs, and favorites David adjusted by hand
 * (`scripts/marketing-shots.ts` § `seedSettingsIfNew`). An unreadable file (a
 * torn write from a killed run) is replaced, since there's nothing to preserve.
 *
 * @param dataDir the instance's `CMDR_DATA_DIR`
 */
export function pinUiLanguage(dataDir: string): void {
  const settingsPath = path.join(dataDir, 'settings.json')

  let settings: Record<string, unknown> = {}
  try {
    const parsed: unknown = JSON.parse(fs.readFileSync(settingsPath, 'utf-8'))
    if (typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)) {
      settings = parsed as Record<string, unknown>
    }
  } catch {
    // No settings file yet, or one no reader could use: start from an empty object.
  }

  settings[LANGUAGE_KEY] = ENGLISH
  fs.mkdirSync(dataDir, { recursive: true })
  fs.writeFileSync(settingsPath, `${JSON.stringify(settings, null, 2)}\n`)
}
