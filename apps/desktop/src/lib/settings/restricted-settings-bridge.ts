/**
 * Restricted-settings bridge - persists settings on behalf of restricted windows.
 *
 * The viewer window has no `tauri-plugin-store` capability (it renders
 * arbitrary, possibly-hostile file content; see
 * `src-tauri/capabilities/CLAUDE.md` § viewer), so it can't save settings
 * itself. Its `setSetting` calls go through the typed
 * `persist_restricted_window_setting` backend command, which validates the
 * setting against an enum allowlist and forwards it here via the
 * `persist-restricted-setting` event. This bridge runs in the main window
 * (always alive) and persists through the normal store pipeline.
 *
 * The allowlist is enforced twice: the backend enum can only express the two
 * permitted settings, and this handler re-checks the id (defense in depth —
 * any webview can emit arbitrary events, so the event payload alone is
 * untrusted).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { persistSettingFromRestrictedWindow } from './settings-store'
import { validateSettingValue } from './settings-registry'
import type { SettingId, SettingsValues } from './types'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('restricted-settings-bridge')

/** Mirrors the backend's `RestrictedWindowPersistableSetting` enum mapping. */
const PERSIST_ALLOWLIST: ReadonlySet<string> = new Set(['viewer.wordWrap', 'fileViewer.suppressBinaryWarning'])

interface PersistRestrictedSettingPayload {
  id: string
  value: unknown
}

let unlisten: UnlistenFn | null = null

/** Handles one forwarded persist request. Exported for unit tests. */
export function handlePersistRestrictedSetting(payload: PersistRestrictedSettingPayload): void {
  const { id, value } = payload
  if (!PERSIST_ALLOWLIST.has(id)) {
    log.warn('Refusing to persist non-allowlisted setting {id} from a restricted window', { id })
    return
  }
  try {
    validateSettingValue(id, value)
    persistSettingFromRestrictedWindow(id as SettingId, value as SettingsValues[SettingId])
    log.debug('Persisted {id} on behalf of a restricted window', { id })
  } catch (error) {
    log.warn('Invalid restricted-window persist request for {id}: {error}', { id, error: String(error) })
  }
}

/** Registers the bridge listener. Call in onMount of the main window. */
export async function setupRestrictedSettingsBridge(): Promise<void> {
  if (unlisten) return
  unlisten = await listen<PersistRestrictedSettingPayload>('persist-restricted-setting', (event) => {
    handlePersistRestrictedSetting(event.payload)
  })
  log.debug('Restricted-settings bridge listener set up')
}

/** Removes the bridge listener. Call in onDestroy of the main window. */
export function cleanupRestrictedSettingsBridge(): void {
  unlisten?.()
  unlisten = null
  log.debug('Restricted-settings bridge listener cleaned up')
}
