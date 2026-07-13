// FE-owned media-index network prefs (plan M1.5): the per-SMB-volume enrichment
// opt-in and the "always index" overrides (per volume and per folder). Each is
// persisted as a real JSON array in the sparse settings store (the Rust loader
// reads `mediaIndex.networkVolumes` / `mediaIndex.alwaysIndexVolumes` /
// `mediaIndex.alwaysIndexFolders` as `Vec<String>`) AND live-applied through the
// matching `media_index_set_*` command, both in one place so the persisted array
// and the running scheduler config never drift.
//
// Why co-locate persist + IPC (not route through `settings-applier.ts`): the
// setters take a per-item delta (`volumeId`, `enabled`), not a whole-array push,
// so they don't fit the applier's key→value passthrough table. This mirrors the
// global-go-to-latest shortcut, which likewise persists then calls its own IPC.

import { getSetting, setSetting } from '$lib/settings'
import { mediaIndexSetNetworkVolumeEnabled, mediaIndexSetAlwaysIndexVolume } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('media-index')

/** Toggle `id` within a JSON-array setting, always replacing by reference so the
 *  store's `===` idempotency guard sees a change and persists. */
function toggleInArray(current: readonly string[], id: string, on: boolean): string[] {
  const has = current.includes(id)
  if (on && !has) return [...current, id]
  if (!on && has) return current.filter((v) => v !== id)
  return [...current]
}

// ── Per-volume network (SMB) opt-in ────────────────────────────────────────

/** Volume ids opted into background network image enrichment. */
export function getNetworkOptInVolumes(): string[] {
  return getSetting('mediaIndex.networkVolumes')
}

/** Whether this network volume is opted into background enrichment. */
export function isNetworkVolumeOptedIn(volumeId: string): boolean {
  return getNetworkOptInVolumes().includes(volumeId)
}

/**
 * Opt a network volume in or out. Persists the array AND live-applies via IPC
 * (enabling kicks an immediate pass backend-side). On IPC failure the persisted
 * choice is rolled back so the UI and backend stay in agreement.
 */
export async function setNetworkVolumeOptedIn(volumeId: string, enabled: boolean): Promise<void> {
  const previous = getNetworkOptInVolumes()
  setSetting('mediaIndex.networkVolumes', toggleInArray(previous, volumeId, enabled))
  try {
    await mediaIndexSetNetworkVolumeEnabled(volumeId, enabled)
  } catch (err) {
    setSetting('mediaIndex.networkVolumes', previous)
    log.warn('Failed to apply network opt-in for {volumeId}: {err}', { volumeId, err: String(err) })
    throw err
  }
}

// ── "Always index" volume override ─────────────────────────────────────────

/** Volume ids marked "always index" (enrich regardless of importance). */
export function getAlwaysIndexVolumes(): string[] {
  return getSetting('mediaIndex.alwaysIndexVolumes')
}

/** Whether this volume is marked "always index". */
export function isVolumeAlwaysIndexed(volumeId: string): boolean {
  return getAlwaysIndexVolumes().includes(volumeId)
}

/** Set (or clear) a whole-volume "always index" override. Persists + live-applies. */
export async function setVolumeAlwaysIndexed(volumeId: string, always: boolean): Promise<void> {
  const previous = getAlwaysIndexVolumes()
  setSetting('mediaIndex.alwaysIndexVolumes', toggleInArray(previous, volumeId, always))
  try {
    await mediaIndexSetAlwaysIndexVolume(volumeId, always)
  } catch (err) {
    setSetting('mediaIndex.alwaysIndexVolumes', previous)
    log.warn('Failed to apply always-index for volume {volumeId}: {err}', { volumeId, err: String(err) })
    throw err
  }
}

// The per-folder "always index" override (setting `mediaIndex.alwaysIndexFolders`,
// backend command `media_index_set_always_index_folder`) is intentionally NOT wired
// on the FE this slice: its natural trigger is a folder right-click action, and the
// file context menu is a native (Rust) menu, so the item + menu-event handler is a
// small backend follow-up. The backend command + the sparse setting are ready.
