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
 * `refreshSystemStrings()` keeps them current when the macOS language moves
 * mid-session.
 */

import { getLocalizedSystemStrings } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { tString } from '$lib/intl/messages.svelte'

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

/** Fetches the snapshot from Rust and writes it into `systemStrings`. */
async function hydrate(): Promise<void> {
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
 * Loads the localized snapshot from Rust and writes it into `systemStrings`.
 * Idempotent. Safe to call multiple times; the second call is a no-op.
 */
export async function initSystemStrings(): Promise<void> {
  if (initialized) return
  await hydrate()
}

/**
 * Re-reads the snapshot after the macOS language moved, so the pane names we
 * quote keep matching what's actually on the user's screen. A no-op in a window
 * that never hydrated: it has no copy quoting these labels, so there's nothing
 * to bring up to date and no reason to spend the round-trip.
 *
 * ❌ Re-READ them; never re-resolve against our own catalog. These follow the
 * SYSTEM language, not `appearance.language` (see the module header), so a user
 * running Cmdr in Spanish on a Hungarian Mac must still be told to open
 * `Rendszerbeállítások`. Rust drops its cache on the same signal
 * (`system_strings.rs::invalidate`), so this really does get the new answer.
 *
 * Callers: the one `watchSystemLocales()` subscriber each window already has
 * (`settings-applier.ts` in the main window, `initWindowLanguageSync()` in the
 * others). ❌ Don't add a second subscriber to call it: the first to run adopts
 * the new locales, so every later subscriber sees no change and never fires.
 */
export async function refreshSystemStrings(): Promise<void> {
  if (!initialized) return
  await hydrate()
}

/**
 * The "this folder is TCC-restricted" tooltip, shared by the sidebar
 * breadcrumb and the file-list rows. Pulled into one place so changes to the
 * wording (or the localized substitutions) happen once. Read from a Svelte
 * `$derived(...)` so updates to `systemStrings` propagate to the tooltip
 * automatically.
 */
export function restrictedFolderTooltip(): string {
  return tString('fileExplorer.restrictedFolder.tooltip', {
    systemSettings: systemStrings.systemSettings,
    privacyAndSecurity: systemStrings.privacyAndSecurity,
    fullDiskAccess: systemStrings.fullDiskAccess,
    filesAndFolders: systemStrings.filesAndFolders,
  })
}
