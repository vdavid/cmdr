/**
 * The two index dots a drive row wears, in both of their placements: next to the
 * breadcrumb chip (the ACTIVE drive) and on each switcher row. One instance serves both,
 * so the event subscriptions, the fetches, and the menu actions happen once.
 *
 * It owns the filesystem freshness statuses (`drive-index-manager.svelte.ts`), the
 * per-drive IMAGE-index states, and what an `enable` / `rescan` / `disable` / `stop` /
 * `forget` pick does. What the dots MEAN is pure and lives elsewhere
 * (`drive-index-status.ts`, `image-index-drive-state.ts`).
 */

import { untrack } from 'svelte'
import { SvelteMap } from 'svelte/reactivity'
import {
  disableDriveIndex,
  enableDriveIndex,
  forgetDriveIndex,
  mediaIndexVolumeState,
  rescanDriveIndex,
  type MediaIndexVolumeState,
} from '$lib/tauri-commands'
import type { DriveIndexRefusal, VolumeIndexStatus } from '$lib/ipc/bindings'
import { getEnrichingVolumes } from '$lib/indexing/media-enrich-state.svelte'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { createDriveIndexManager, isDriveRow } from './drive-index-manager.svelte'
import { driveIndexActionFeedback, driveIndexRefusalMessageKey, type DriveIndexMenuAction } from './drive-index-status'
import { connectDirectlyToRow } from './connect-directly-row'
import type { VolumeInfo } from '../types'

export interface DriveBadgesDeps {
  /** The full store volume list, for naming a drive in a toast and in the connect flow. */
  getVolumes: () => VolumeInfo[]
  /** The volume the pane is on: its badges refetch whenever it changes. */
  getActiveVolume: () => VolumeInfo | undefined
}

export interface DriveBadges {
  /** The filesystem freshness status for a drive, or nothing to show yet. */
  statusFor: (volumeId: string) => VolumeIndexStatus | undefined
  /** The image-index state for a drive, or nothing to show yet. */
  imageStateFor: (volumeId: string) => MediaIndexVolumeState | undefined
  /** Fill both maps for the rows a just-opened switcher shows. */
  fetchForRows: (volumes: VolumeInfo[]) => void
  /** Run a badge-menu pick. Sync, because the badge's `onAction` prop returns void. */
  runAction: (volumeId: string, action: DriveIndexMenuAction) => void
  destroy: () => void
}

export function createDriveBadges(deps: DriveBadgesDeps): DriveBadges {
  const driveIndex = createDriveIndexManager()

  // Fetched lazily (active drive always, switcher rows on open) and refreshed on that
  // volume's enrich events — bounded to the handful of shown drives, so no poll.
  // `ImageIndexDriveBadge` hides itself on drives with no images.
  const imageIndexStateMap = new SvelteMap<string, MediaIndexVolumeState>()

  async function fetchImageIndexState(volumeId: string): Promise<void> {
    try {
      imageIndexStateMap.set(volumeId, await mediaIndexVolumeState(volumeId))
    } catch {
      // Media index not ready / unavailable: leave the dot hidden until a later fetch.
    }
  }

  // Keep the always-visible active-drive badges fresh: refetch whenever the active drive
  // changes. Later updates arrive through the manager's event subscriptions and the
  // enrichment feed below (subscribe, don't poll).
  $effect(() => {
    const active = deps.getActiveVolume()
    if (!active || !isDriveRow(active)) return
    void driveIndex.fetchStatus(active.id)
    void fetchImageIndexState(active.id)
  })

  // Keep the tracked drives' image-index counts live: refetch whenever ANY volume's
  // enrichment activity changes (a pass starting, ticking, or ending). Reads the GLOBAL
  // `media-enrich-state` reactivity (app-wide, one publisher), so no `media-enrich-*`
  // listeners of our own. The map read is untracked so re-setting it here doesn't
  // re-trigger this effect.
  $effect(() => {
    getEnrichingVolumes() // reactive dep: re-runs on any enrichment-activity change
    for (const volumeId of untrack(() => [...imageIndexStateMap.keys()])) {
      void fetchImageIndexState(volumeId)
    }
  })

  /** Surface a typed index refusal: route credentials to login, else a friendly toast.
   *  The variant→copy mapping is the pure `driveIndexRefusalMessageKey` (unit-tested). */
  function handleRefusal(volumeId: string, name: string, reason: DriveIndexRefusal): void {
    const messageKey = driveIndexRefusalMessageKey(reason)
    if (messageKey === null) {
      // `credentials_needed`: reuse the direct-connect flow, which prompts for the
      // password and reconnects; the user can then turn indexing on again.
      void connectDirectlyToRow(volumeId, deps.getVolumes())
      return
    }
    addToast(tString(messageKey, { name }), { level: 'error' })
  }

  // `enable`/`rescan` can be refused on SMB (a typed `DriveIndexRefusal`); we classify by
  // variant (never by message) and route `credentials_needed` into the direct-connect flow.
  async function runActionAsync(volumeId: string, action: DriveIndexMenuAction): Promise<void> {
    const name = deps.getVolumes().find((volume) => volume.id === volumeId)?.name ?? volumeId
    try {
      if (action === 'forget') {
        // Delete the drive's index DB entirely (vs disable, which keeps it on disk to
        // resume). The recovery path for an index stuck in a bad state. Its badge goes
        // gray; re-enabling does a fresh full scan.
        await forgetDriveIndex(volumeId)
      } else if (action === 'disable' || action === 'stop') {
        await disableDriveIndex(volumeId)
      } else {
        // enable | rescan: both return an EnableIndexingOutcome, and what it's worth
        // telling the user is the pure, unit-tested `driveIndexActionFeedback` — this
        // only carries the answer out.
        const outcome = action === 'enable' ? await enableDriveIndex(volumeId) : await rescanDriveIndex(volumeId)
        const feedback = driveIndexActionFeedback(action, outcome)
        if (feedback.kind === 'refusal') {
          handleRefusal(volumeId, name, feedback.reason)
        } else if (feedback.kind === 'toast') {
          addToast(tString(feedback.key, { name }), { level: feedback.level })
        }
      }
    } catch {
      addToast(tString('fileExplorer.navigation.driveIndex.refusedGeneric', { name }), { level: 'error' })
    }
    await driveIndex.fetchStatus(volumeId)
  }

  return {
    statusFor: (volumeId) => driveIndex.statusMap.get(volumeId),
    imageStateFor: (volumeId) => imageIndexStateMap.get(volumeId),
    fetchForRows(volumes) {
      void driveIndex.fetchStatuses(volumes)
      for (const volume of volumes) {
        if (isDriveRow(volume) && !imageIndexStateMap.has(volume.id)) void fetchImageIndexState(volume.id)
      }
    },
    runAction(volumeId, action) {
      void runActionAsync(volumeId, action)
    },
    destroy: driveIndex.destroy,
  }
}
