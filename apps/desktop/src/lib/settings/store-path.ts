// eslint-disable-next-line cmdr/no-raw-bindings-import -- logging/store bootstrap infra: the tauri-commands barrel imports the logger (storage.ts), so wrapping here would create an import cycle
import { commands } from '$lib/ipc/bindings'

/**
 * Resolve the path `tauri-plugin-store` should load for a given store file
 * (for example `settings.json`, `shortcuts.json`, `app-status.json`).
 *
 * In isolated instances (dev, per-worktree dev, E2E — anything that sets
 * `CMDR_DATA_DIR`), the backend returns an absolute path under the resolved
 * data dir so the frontend store agrees with the backend. Without this, the
 * store would resolve a bare name via Tauri's identifier-driven
 * `app_data_dir()`, which ignores `CMDR_DATA_DIR` and lands on the real
 * production file — leaking the developer's local state into dev/E2E.
 *
 * In production (`CMDR_DATA_DIR` unset) the command returns `null` and this
 * returns the bare `storeName`, byte-identical to loading it directly. The
 * backend also returns `null` for any name it can't safely place inside the
 * data dir (path traversal, absolute paths), so a bad name degrades to the
 * production path rather than escaping the data dir.
 *
 * `onError` lets callers log in their own style (the logger initializes before
 * its own app logger exists, so it can't depend on `$lib/logging`).
 */
export async function resolveStorePath(storeName: string, onError?: (error: unknown) => void): Promise<string> {
  try {
    const isolated = await commands.getIsolatedStorePath(storeName)
    if (isolated) return isolated
  } catch (e) {
    onError?.(e)
  }
  return storeName
}
