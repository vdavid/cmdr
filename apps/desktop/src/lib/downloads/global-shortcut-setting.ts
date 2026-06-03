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
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- key is in the registry
  return getSetting(ENABLED_KEY as any) as boolean
}

export function getGlobalGoToLatestBinding(): string {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- key is in the registry
  return getSetting(BINDING_KEY as any) as string
}

export function setGlobalGoToLatestEnabled(value: boolean): void {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- key is in the registry
  setSetting(ENABLED_KEY as any, value)
}

/**
 * Set the binding AND reset `acknowledged` to `false`. The reset is the
 * whole point of this helper — see the module docstring.
 */
export function setGlobalGoToLatestBinding(value: string): void {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- key is in the registry
  setSetting(BINDING_KEY as any, value)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- key is in the registry
  setSetting(ACKNOWLEDGED_KEY as any, false)
}

export function setGlobalGoToLatestAcknowledged(value: boolean): void {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- key is in the registry
  setSetting(ACKNOWLEDGED_KEY as any, value)
}
