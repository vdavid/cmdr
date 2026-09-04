/**
 * Pure option-building for the "Open terminal here uses" row, kept beside
 * `TerminalAppSelect.svelte` so the list rules are testable without a DOM.
 *
 * The backend answers "which terminals are installed, and which one is chosen"
 * (`list_terminal_apps`); everything here is presentation of that answer.
 */

import type { TerminalApp, TerminalAppList } from '$lib/ipc/bindings'

/**
 * One dropdown row. Structurally a `SelectItem` (`$lib/ui/Select.svelte`), spelled
 * out here rather than imported: this module is plain TypeScript, and a type
 * reaching out of a `.svelte` module block doesn't resolve for the TS-aware lint
 * pass over `.ts` files.
 */
export interface TerminalAppOption {
  value: string
  label: string
  iconUrl?: string
}

/**
 * Terminal.app's bundle id, the `behavior.openTerminalHereApp` default. It ships
 * with macOS, so it's also what the row shows once the chosen app is gone.
 * Mirrors `TERMINAL_APP_BUNDLE_ID` in `src-tauri/src/file_system/terminal.rs`.
 */
export const TERMINAL_APP_BUNDLE_ID = 'com.apple.Terminal'

/**
 * The "Choose an app…" row's value. Never stored: the row intercepts it and
 * opens the app picker instead. It's neither a bundle id nor an absolute path,
 * the two shapes Rust's `parse_choice` reads, so it can't be mistaken for a
 * real choice even if it somehow reached the store.
 */
export const CHOOSE_APP_VALUE = '__choose_app__'

/**
 * The dropdown rows: every installed terminal in the order the backend listed
 * them (its own table order, custom pick last), then "Choose an app…".
 * @param apps - What `list_terminal_apps` found installed.
 * @param chooseAppLabel - The resolved "Choose an app…" label.
 */
export function terminalAppItems(apps: TerminalApp[], chooseAppLabel: string): TerminalAppOption[] {
  const items: TerminalAppOption[] = apps.map((app) => ({
    value: app.id,
    label: app.displayName,
    iconUrl: app.icon ?? undefined,
  }))
  items.push({ value: CHOOSE_APP_VALUE, label: chooseAppLabel })
  return items
}

/**
 * Which row reads as selected.
 *
 * A `chosenId` of `null` means the stored app has been uninstalled. The action
 * itself falls back to Terminal in that case, so the row says the same thing
 * rather than showing an empty control for an app that isn't there. It only
 * DISPLAYS the fallback: rewriting the setting is the action's job, at the
 * moment it actually opens Terminal instead.
 * @param list - The backend's answer.
 * @returns The row's value, or an empty string while the list is still empty.
 */
export function selectedTerminalAppId(list: TerminalAppList): string {
  if (list.chosenId !== null) return list.chosenId
  return list.apps.some((app) => app.id === TERMINAL_APP_BUNDLE_ID) ? TERMINAL_APP_BUNDLE_ID : ''
}
