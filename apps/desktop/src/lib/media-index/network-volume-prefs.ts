// FE-owned media-index network prefs (network enrichment): the per-SMB-volume enrichment
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

/** The media-index settings that hold a JSON array of ids or folder paths. */
type ArraySettingId =
  | 'mediaIndex.networkVolumes'
  | 'mediaIndex.alwaysIndexVolumes'
  | 'mediaIndex.alwaysIndexFolders'
  | 'mediaIndex.excludedFolders'

/** One per-item change to an array setting, and what a failed apply does to it. */
interface ArrayDelta {
  setting: ArraySettingId
  id: string
  on: boolean
  /**
   * Whether a failed apply takes the persisted change back. False only for a change the backend may already be
   * acting on when the call rejects (the exclusion veto), where the saved value has to keep matching it.
   */
  rollback: boolean
}

/**
 * Persists one per-item change, then live-applies it through `apply`. A failed apply with `rollback` reverts only
 * `id`'s own membership, read against the array as it is NOW: restoring a snapshot taken before the await would also
 * undo any toggle that landed meanwhile. Rethrows, so the caller keeps its own log line.
 */
export async function persistThenApply(delta: ArrayDelta, apply: () => Promise<void>): Promise<void> {
  const { setting, id, on, rollback } = delta
  const wasOn = getSetting(setting).includes(id)
  setSetting(setting, toggleInArray(getSetting(setting), id, on))
  try {
    await apply()
  } catch (err) {
    if (rollback && wasOn !== on) setSetting(setting, toggleInArray(getSetting(setting), id, wasOn))
    throw err
  }
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
 * (enabling kicks an immediate pass backend-side). On IPC failure this volume's
 * persisted choice is rolled back so the UI and backend stay in agreement.
 */
export async function setNetworkVolumeOptedIn(volumeId: string, enabled: boolean): Promise<void> {
  try {
    await persistThenApply({ setting: 'mediaIndex.networkVolumes', id: volumeId, on: enabled, rollback: true }, () =>
      mediaIndexSetNetworkVolumeEnabled(volumeId, enabled),
    )
  } catch (err) {
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
  try {
    await persistThenApply({ setting: 'mediaIndex.alwaysIndexVolumes', id: volumeId, on: always, rollback: true }, () =>
      mediaIndexSetAlwaysIndexVolume(volumeId, always),
    )
  } catch (err) {
    log.warn('Failed to apply always-index for volume {volumeId}: {err}', { volumeId, err: String(err) })
    throw err
  }
}

// ── A volume whose id changed ───────────────────────────────────────────────

/**
 * Carries every per-VOLUME choice above from `oldId` to `newId`: a saved server moved,
 * so its place has a new id (an SMB share at its first mount at the new address,
 * `src-tauri/src/server_move_smb.rs`). Without it the share's photos quietly stop
 * enriching. The new id is set before the old one is cleared, so a failure between the
 * two leaves the choice doubled rather than lost. A choice the volume didn't have, or a
 * move that kept the id, touches nothing.
 *
 * Folder overrides are OS paths under the mount, which a move normally keeps; they
 * stay as they are.
 */
export async function followVolumeMove(oldId: string, newId: string): Promise<void> {
  if (oldId === newId) return
  if (isNetworkVolumeOptedIn(oldId)) {
    await setNetworkVolumeOptedIn(newId, true)
    await setNetworkVolumeOptedIn(oldId, false)
  }
  if (isVolumeAlwaysIndexed(oldId)) {
    await setVolumeAlwaysIndexed(newId, true)
    await setVolumeAlwaysIndexed(oldId, false)
  }
}

// The per-folder "always index" override lives in `always-index-folders.ts`.
