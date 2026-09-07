/**
 * A pane standing on a phone, and what it takes to open one.
 *
 * The device twin of `place-connect.svelte.ts`: same seam, same typed
 * `RemoteConnectState`, same one-per-landing rule. What differs is the gate — a
 * server's is its CONNECTION STATE, a phone's is its `deviceReadiness`, because
 * a phone waiting for its "Allow USB debugging?" tap has no session to be in any
 * state about (`$lib/adb/device-readiness.ts` carries that split).
 *
 * ❗ **This factory holds the pane's listing while it works** (`holdsListing`).
 * Without that, `list_directory` reaches `resolve_path_to_volume`, which dials
 * the same phone AGAIN under the backend's own `adb-navigation:<serial>` attempt
 * id — and Cancel, which aims at the id minted here, would then call off the one
 * dial nobody was watching while the other quietly registered the volume.
 *
 * ❗ **The auto-proceed is a READ, not a listener.** `getCurrentVolumeInfo()` is
 * the pane's own lookup into the volume store, and the store is what subscribes
 * to `volumes-changed`. So a phone turning ready re-runs the effect below and
 * the dial starts with nothing pressed — one subscription for the whole app
 * rather than one per pane.
 *
 * MTP is deliberately NOT folded in here: its volume id CHANGES on connect
 * (device-only → storage), which is a different pane transition, and it keeps
 * `MtpConnectionView.svelte`.
 */

import { asAdbConnectError, cancelAdbConnect, connectAdbDevice, newAdbAttemptId } from '$lib/tauri-commands'
import { openSettingsWindow } from '$lib/settings/settings-window'
import { isAdbVolumeId, parseAdbPath } from '$lib/adb/adb-path-utils'
import { readAdbConnectOutcome, waitingForTheAllowTap } from '$lib/adb/adb-connect-errors'
import { getAppLogger } from '$lib/logging/logger'
import type { RemoteConnectState } from './remote-connect-state'
import type { VolumeInfo } from '../types'

const log = getAppLogger('adb')

/** Where Settings keeps the ADB controls, for the one refusal only Settings can clear. */
const ADB_SETTINGS_SECTION = ['File systems', 'Android (ADB)']

export interface DeviceConnectDeps {
  /** The pane's volume id. */
  getVolumeId: () => string
  /** The pane's live `VolumeInfo` (or null). Owned by the component. */
  getCurrentVolumeInfo: () => VolumeInfo | null
  /**
   * The phone is open now. The pane re-runs its listing, which is what turns the
   * connecting view into a directory.
   */
  onConnected: (volumeId: string) => void
}

export interface DeviceConnect {
  /** What the pane renders instead of a listing, or `null` for a normal pane. */
  readonly state: RemoteConnectState | null
  /**
   * Whether the pane must NOT run its own `loadDirectory` yet.
   *
   * ❗ A `$derived` off the volume id and this factory's own record, ❌ never off
   * `state`: the two gates would then depend on `$effect` ordering, and the
   * pane's mount-time load runs before this factory's effect has said anything.
   */
  readonly holdsListing: boolean
}

export function createDeviceConnect(deps: DeviceConnectDeps): DeviceConnect {
  let state = $state<RemoteConnectState | null>(null)
  /** The volume this factory has seen all the way open. */
  let opened = $state<string | null>(null)
  /** The attempt a Cancel aims at. Plain, not `$state`: nothing renders it. */
  let attemptId: string | null = null
  /**
   * The `<volume>:<readiness>` pair already acted on, so one landing is one dial
   * and a readiness CHANGE (the Allow tap) is a fresh decision.
   */
  let handled: string | null = null

  const holdsListing = $derived(
    isAdbVolumeId(deps.getVolumeId()) && deps.getCurrentVolumeInfo() !== null && opened !== deps.getVolumeId(),
  )

  $effect(() => {
    const volumeId = deps.getVolumeId()
    const info = deps.getCurrentVolumeInfo()
    if (!isAdbVolumeId(volumeId) || !info) {
      // A local volume, a server, or a phone that left the list. Nothing to hold
      // and nothing to remember: a phone that comes back is dialed afresh.
      state = null
      attemptId = null
      handled = null
      opened = null
      return
    }
    const key = `${volumeId}:${readinessKey(info)}`
    if (handled === key) return
    handled = key
    if (info.deviceReadiness?.kind === 'waiting_for_authorization') {
      // ❗ ❌ No dial: the answer is `unauthorized` and it is already on screen.
      // The next `volumes-changed` carrying `ready` re-runs this effect.
      state = waiting()
      return
    }
    void dial(volumeId, info)
  })

  /** What the effect keys on, so a readiness change is a new decision. */
  function readinessKey(info: VolumeInfo): string {
    const readiness = info.deviceReadiness
    if (!readiness) return 'none'
    return readiness.kind === 'unavailable' ? `unavailable:${readiness.reason}` : readiness.kind
  }

  function waiting(): RemoteConnectState {
    const words = waitingForTheAllowTap()
    return {
      kind: 'waiting_for_device',
      reason: words.reason,
      hint: words.hint,
      cancel: () => {
        // A dial may be in flight (the `unauthorized` answer arrives here too),
        // and calling one off that already finished is an ordinary `false`.
        if (attemptId) void callOff(attemptId)
        state = null
      },
    }
  }

  async function dial(volumeId: string, info: VolumeInfo): Promise<void> {
    const parsed = parseAdbPath(info.path)
    if (!parsed) {
      // A device row whose path isn't `adb://…`. Nothing to dial and nothing to
      // say: the listing runs and answers honestly.
      log.warn('An ADB volume names no serial in its path: {path}', { path: info.path })
      state = null
      opened = volumeId
      return
    }
    // ❗ Minted BEFORE the wire is touched, so Cancel is armed from the first
    // millisecond: a phone can sit on its prompt for as long as nobody picks up.
    const id = newAdbAttemptId()
    attemptId = id
    state = {
      kind: 'connecting',
      cancel: () => {
        void callOff(id)
        state = null
      },
    }
    try {
      await connectAdbDevice(parsed.serial, id)
      attemptId = null
      opened = volumeId
      state = null
      deps.onConnected(volumeId)
    } catch (e) {
      attemptId = null
      state = stateForRefusal(volumeId, info, e)
    }
  }

  /** What the pane shows for a dial that came back with something to say. */
  function stateForRefusal(volumeId: string, info: VolumeInfo, error: unknown): RemoteConnectState | null {
    const failure = asAdbConnectError(error)
    if (!failure) {
      // Not a typed refusal, so it is the IPC transport itself. ❌ Never shown:
      // it is untranslated diagnostic text. The listing's own error pane takes
      // over once the pane stops being held.
      log.warn('Opening the phone on {volumeId} broke down: {error}', { volumeId, error: String(error) })
      opened = volumeId
      return null
    }
    const outcome = readAdbConnectOutcome(failure)
    switch (outcome.kind) {
      case 'silent':
        // ❗ Says nothing: the user pressed the button.
        return null
      case 'waiting':
        return waiting()
      case 'refused':
        return {
          kind: 'refused',
          refusal: outcome.sentence,
          retry: outcome.recovery === 'retry' ? () => void dial(volumeId, info) : undefined,
          openSettings:
            outcome.recovery === 'open_settings'
              ? () => void openSettingsWindow('adb-refusal', ADB_SETTINGS_SECTION)
              : undefined,
        }
    }
  }

  async function callOff(id: string): Promise<void> {
    try {
      await cancelAdbConnect(id)
    } catch (e) {
      log.warn('Calling off the ADB connect {id} broke down: {error}', { id, error: String(e) })
    }
  }

  return {
    get state() {
      return state
    },
    get holdsListing() {
      return holdsListing
    },
  }
}
