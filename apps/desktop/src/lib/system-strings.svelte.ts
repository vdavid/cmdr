/**
 * Localized macOS system pane labels for user-facing copy.
 *
 * The app's UI language is independent of the macOS UI language: a user can
 * run Cmdr in English on a Hungarian macOS, or in Spanish on a French macOS.
 * Onboarding and friendly-error copy point users at specific System Settings
 * panes ("Full Disk Access", "Privacy & Security"), so the labels we render
 * must match what's actually on screen in System Settings, not the app's own
 * language. Backend reads the strings from `.loctable` files in system
 * bundles; see `apps/desktop/src-tauri/src/system_strings.rs` for details.
 *
 * Usage:
 *
 * ```svelte
 * <script>
 *   import { systemStrings } from '$lib/system-strings.svelte'
 * </script>
 * <p>Open {systemStrings.systemSettings} &gt; {systemStrings.privacyAndSecurity}.</p>
 * ```
 *
 * For Rust-rendered markdown that ships from the backend, the `expand` helper
 * here mirrors the backend's `system_strings::expand` for any frontend
 * template strings that need the same substitution.
 *
 * Call `initSystemStrings()` once at startup. Until then, `systemStrings`
 * holds the English defaults so SSR / first-render still produce correct copy.
 */

import { getLocalizedSystemStrings } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('system-strings')

/** English defaults. Mirrors `LocalizedSystemStrings::english_defaults` in Rust. */
const ENGLISH_DEFAULTS = {
  systemSettings: 'System Settings',
  privacyAndSecurity: 'Privacy & Security',
  fullDiskAccess: 'Full Disk Access',
  filesAndFolders: 'Files & Folders',
  localNetwork: 'Local Network',
  appearance: 'Appearance',
}

/**
 * Reactive snapshot. Pre-populated with English defaults so renders before
 * `initSystemStrings()` resolves are still grammatically correct (just not
 * localized).
 */
export const systemStrings = $state({ ...ENGLISH_DEFAULTS })

let initialized = false

/**
 * Loads the localized snapshot from Rust and writes it into `systemStrings`.
 * Idempotent. Safe to call multiple times; the second call is a no-op.
 */
export async function initSystemStrings(): Promise<void> {
  if (initialized) return
  try {
    const resolved = await getLocalizedSystemStrings()
    systemStrings.systemSettings = resolved.systemSettings
    systemStrings.privacyAndSecurity = resolved.privacyAndSecurity
    systemStrings.fullDiskAccess = resolved.fullDiskAccess
    systemStrings.filesAndFolders = resolved.filesAndFolders
    systemStrings.localNetwork = resolved.localNetwork
    systemStrings.appearance = resolved.appearance
    initialized = true
    log.debug('System strings hydrated: {systemSettings}, {fullDiskAccess}', {
      systemSettings: resolved.systemSettings,
      fullDiskAccess: resolved.fullDiskAccess,
    })
  } catch (error) {
    log.warn('Failed to load localized system strings, falling back to English: {error}', { error })
  }
}

/**
 * The "this folder is TCC-restricted" tooltip, shared by the sidebar
 * breadcrumb and the file-list rows. Pulled into one place so changes to the
 * wording (or the localized substitutions) happen once. Read from a Svelte
 * `$derived(...)` so updates to `systemStrings` propagate to the tooltip
 * automatically.
 */
export function restrictedFolderTooltip(): string {
  const s = systemStrings
  return (
    `Access to this folder is limited. ` +
    `Grant Cmdr ${s.fullDiskAccess} in ${s.systemSettings} → ${s.privacyAndSecurity} → ${s.fullDiskAccess} ` +
    `to remove all such limits. ` +
    `Or grant per-folder access in ${s.systemSettings} → ${s.privacyAndSecurity} → ${s.filesAndFolders} → Cmdr.`
  )
}
