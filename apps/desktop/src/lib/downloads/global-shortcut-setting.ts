/**
 * Settings helpers for `behavior.fileSystemWatching.globalGoToLatestShortcut.*`.
 *
 * The three settings (`enabled`, `binding`, `acknowledged`) live in the
 * registry and persist like everything else. This module exposes the
 * narrowed setters with the "reset acknowledged on binding change" rule
 * baked in so call sites can't forget.
 *
 * Why a separate setter for `binding`: the warn-toast suppression is keyed
 * on `acknowledged`. When the user picks a different combo we treat it as a
 * fresh hotkey — the new combo deserves the first-trigger warning again, so
 * we reset `acknowledged` whenever the binding changes. Centralizing this in
 * one place keeps the Settings UI and tests from drifting.
 */
import { getSetting, setSetting } from '$lib/settings'

const ENABLED_KEY = 'behavior.fileSystemWatching.globalGoToLatestShortcut.enabled'
const BINDING_KEY = 'behavior.fileSystemWatching.globalGoToLatestShortcut.binding'
const ACKNOWLEDGED_KEY = 'behavior.fileSystemWatching.globalGoToLatestShortcut.acknowledged'

export {
  ENABLED_KEY as GLOBAL_GO_TO_LATEST_ENABLED_KEY,
  BINDING_KEY as GLOBAL_GO_TO_LATEST_BINDING_KEY,
  ACKNOWLEDGED_KEY as GLOBAL_GO_TO_LATEST_ACKNOWLEDGED_KEY,
}

export function getGlobalGoToLatestEnabled(): boolean {
  return getSetting(ENABLED_KEY)
}

export function getGlobalGoToLatestBinding(): string {
  return getSetting(BINDING_KEY)
}

export function setGlobalGoToLatestEnabled(value: boolean): void {
  setSetting(ENABLED_KEY, value)
}

/**
 * Set the binding AND reset `acknowledged` to `false`. The reset is the
 * whole point of this helper — see the module docstring.
 */
export function setGlobalGoToLatestBinding(value: string): void {
  setSetting(BINDING_KEY, value)
  setSetting(ACKNOWLEDGED_KEY, false)
}

export function setGlobalGoToLatestAcknowledged(value: boolean): void {
  setSetting(ACKNOWLEDGED_KEY, value)
}
