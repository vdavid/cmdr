import { commands } from '$lib/ipc/bindings'

/** Bare store name. `tauri-plugin-store` resolves this against `BaseDirectory::AppData`. */
export const SETTINGS_STORE_NAME = 'settings.json'

/**
 * Resolve the path `tauri-plugin-store` should load for `settings.json`.
 *
 * In isolated instances (dev, per-worktree dev, E2E — anything that sets
 * `CMDR_DATA_DIR`), the backend returns an absolute path under the resolved
 * data dir so the frontend store agrees with the Rust loader
 * (`settings::load_settings`). Without this, the store would resolve
 * `settings.json` via Tauri's identifier-driven `app_data_dir()`, which ignores
 * `CMDR_DATA_DIR` and lands on the real production file — leaking the
 * developer's local settings into dev/E2E.
 *
 * In production (`CMDR_DATA_DIR` unset) the command returns `null` and this
 * returns the bare `'settings.json'`, byte-identical to loading it directly.
 *
 * `onError` lets callers log in their own style (the logger initializes before
 * its own app logger exists, so it can't depend on `$lib/logging`).
 */
export async function resolveSettingsStorePath(onError?: (error: unknown) => void): Promise<string> {
  try {
    const isolated = await commands.getIsolatedSettingsPath()
    if (isolated) return isolated
  } catch (e) {
    onError?.(e)
  }
  return SETTINGS_STORE_NAME
}
