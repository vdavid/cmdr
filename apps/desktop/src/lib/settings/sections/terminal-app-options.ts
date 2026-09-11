/**
 * Pure option-building for the "Open terminal here uses" row, kept beside
 * `TerminalAppSelect.svelte` so the list rules are testable without a DOM.
 *
 * The backend answers "which terminals are installed, and which one is chosen"
 * (`list_terminal_apps`); everything here is presentation of that answer.
 */

import type { TerminalApp, TerminalAppList } from '$lib/ipc/bindings'
import { CHOOSE_APP_VALUE, type AppChoiceOption } from './app-choice-options'

export { CHOOSE_APP_VALUE }

/**
 * Terminal.app's bundle id, the `behavior.openTerminalHereApp` default. It ships
 * with macOS, so it's also what the row shows once the chosen app is gone.
 * Mirrors `TERMINAL_APP_BUNDLE_ID` in `src-tauri/src/file_system/terminal.rs`.
 */
export const TERMINAL_APP_BUNDLE_ID = 'com.apple.Terminal'

/**
 * The dropdown rows: every installed terminal in the order the backend listed
 * them (its own table order, custom pick last), then "Choose an app…".
 * @param apps - What `list_terminal_apps` found installed.
 * @param chooseAppLabel - The resolved "Choose an app…" label.
 */
export function terminalAppItems(apps: TerminalApp[], chooseAppLabel: string): AppChoiceOption[] {
  const items: AppChoiceOption[] = apps.map((app) => ({
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
