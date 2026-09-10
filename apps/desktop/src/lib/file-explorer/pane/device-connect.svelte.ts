/**
 * A pane standing on a phone, and what it takes to open one.
 *
 * The device twin of `place-connect.svelte.ts`: same seam, same typed
 * `RemoteConnectState`, same one-per-landing rule. What differs is the gate — a
 * server's is its CONNECTION STATE, a phone's is its `deviceReadiness`, because
 * a phone waiting for its "Allow USB debugging?" tap has no session to be in any
 * state about (`$lib/adb/device-readiness.ts` carries that split).
 *
 * ❗ **This factory is the ONE dialer, and it holds the pane's listing while it
 * works** (`holdsListing`). Path resolution never dials, and a phone nobody has
 * dialed has no registered volume, so a listing there can only come back refused
 * (`DeviceDisconnected`). The reload on connect is what lists the phone.
 *
 * ❗ **"Open" is re-checked against the row.** Enrichment fills `capabilities`
 * only for a registered volume, so a phone that stays listed after an eject
 * comes back without them, and the factory holds the listing and dials again.
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

import { untrack } from 'svelte'
import { asAdbConnectError, cancelAdbConnect, connectAdbDevice, newAdbAttemptId } from '$lib/tauri-commands'
import { openSettingsWindow } from '$lib/settings/settings-window'
import { isAdbVolumeId, parseAdbPath } from '$lib/adb/adb-path-utils'
import { readAdbConnectOutcome, waitingForTheAllowTap } from '$lib/adb/adb-connect-errors'
import { tString } from '$lib/intl/messages.svelte'
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
  /**
   * Whether a row has shown `opened` REGISTERED (carrying `capabilities`) since
   * the dial. Plain, not `$state`: only the effect reads it. ❗ A fresh dial's
   * broadcast can land after the dial answers, so a row without `capabilities`
   * counts as an eject only once one with them was seen.
   */
  let seenRegistered = false
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
      seenRegistered = false
      return
    }
    if (untrack(() => opened) === volumeId) {
      if (info.capabilities != null) {
        seenRegistered = true
      } else if (seenRegistered) {
        // Retired while still listed (an eject). A listing could only come back
        // refused now, so hold it again and let the decision below dial.
        opened = null
        seenRegistered = false
        handled = null
      }
    }
    const key = `${volumeId}:${readinessKey(info)}`
    if (handled === key) return
    handled = key
    if (info.deviceReadiness?.kind === 'waiting_for_authorization') {
      // ❗ ❌ No dial: the answer is `unauthorized` and it is already on screen.
      // The next `volumes-changed` carrying `ready` re-runs this effect.
      state = waiting(volumeId, info)
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

  function waiting(volumeId: string, info: VolumeInfo): RemoteConnectState {
    const words = waitingForTheAllowTap()
    return {
      kind: 'waiting_for_device',
      reason: words.reason,
      hint: words.hint,
      cancel: () => {
        // A dial may be in flight (the `unauthorized` answer arrives here too),
        // and calling one off that already finished is an ordinary `false`.
        if (attemptId) void callOff(attemptId)
        state = stopped(volumeId, info)
      },
    }
  }

  /**
   * What a phone the user stopped opening leaves on screen.
   *
   * ❗ ❌ Never `null`. The pane is still HELD while this factory is on a phone
   * it hasn't seen open (`holdsListing`), so a `null` state renders NOTHING at
   * all: no listing, no sentence, no button, and no way out but switching
   * volumes. "A cancel says nothing" is about not scolding the user for what
   * they just did, ❌ not about leaving them in an empty pane.
   *
   * ❗ ❌ And never a release of the hold either: nothing re-runs the listing on
   * that path, and one that ran could only come back refused, because the
   * phone the user just called off has no volume.
   */
  function stopped(volumeId: string, info: VolumeInfo): RemoteConnectState {
    return {
      kind: 'refused',
      refusal: tString('adb.connect.cancelled'),
      retry: () => void dial(volumeId, info),
    }
  }

  async function dial(volumeId: string, info: VolumeInfo): Promise<void> {
    const parsed = parseAdbPath(info.path)
    if (!parsed) {
      // A device row whose path isn't `adb://…`, so there is no serial to dial.
      // The reason is a bug in whoever minted the row, which is what the log is
      // for; what a person can act on is that this row won't open, and no button
      // changes that. ❗ A sentence rather than an empty pane: the listing is
      // still held, so `null` here would render nothing at all.
      log.warn('An ADB volume names no serial in its path: {path}', { path: info.path })
      state = { kind: 'refused', refusal: tString('adb.connect.deviceGone') }
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
        state = stopped(volumeId, info)
      },
    }
    try {
      await connectAdbDevice(parsed.serial, id)
      attemptId = null
      opened = volumeId
      // A phone another pane already opened may never broadcast again, so what
      // its row says now is the baseline an eject is measured against.
      seenRegistered = deps.getCurrentVolumeInfo()?.capabilities != null
      state = null
      deps.onConnected(volumeId)
    } catch (e) {
      attemptId = null
      state = stateForRefusal(volumeId, info, e)
    }
  }

  /** What the pane shows for a dial that came back with something to say. */
  function stateForRefusal(volumeId: string, info: VolumeInfo, error: unknown): RemoteConnectState {
    const failure = asAdbConnectError(error)
    if (!failure) {
      // Not a typed refusal, so it is the IPC transport itself. ❌ Its own text
      // is never shown: it is untranslated diagnostics, and the log is where
      // that belongs. The pane gets the one sentence true either way, with a
      // Try again beside it — ❌ never an empty held pane.
      log.warn('Opening the phone on {volumeId} broke down: {error}', { volumeId, error: String(error) })
      return {
        kind: 'refused',
        refusal: tString('adb.connect.transport'),
        retry: () => void dial(volumeId, info),
      }
    }
    const outcome = readAdbConnectOutcome(failure)
    switch (outcome.kind) {
      case 'silent':
        // The user pressed the button. ❗ No scolding, but a way back in.
        return stopped(volumeId, info)
      case 'waiting':
        return waiting(volumeId, info)
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
