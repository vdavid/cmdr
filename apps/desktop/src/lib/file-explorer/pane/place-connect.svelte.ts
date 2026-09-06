/**
 * A pane standing on a saved place, and the dial that brings it to life.
 *
 * ❗ **Opening a place that isn't live brings it to life IN THE PANE, with a
 * cancel** (`docs/specs/servers-hub-plan.md` § "The four rules"). MTP already
 * works this way; this is the same shape for a server, so activating a greyed
 * `saved` row in the switcher costs one keystroke and shows what it is doing.
 *
 * A `*.svelte.ts` factory rather than lines in `FilePane.svelte`, which is
 * `file-length`-flagged: the pane keeps a two-line branch and this owns the
 * `$effect`, the attempt id, and the words.
 */

import { connectPlace, cancelPlaceConnect } from '$lib/servers/connect-flow'
import { openSignInForPlace } from '$lib/servers/open-sign-in'
import type { ConnectRefusalKind } from '$lib/servers/connect-refusals'
import { wordConnectRefusal } from '$lib/servers/connect-refusals'
import { parseServerPath } from '$lib/servers/server-path-utils'
import { getAppLogger } from '$lib/logging/logger'
import type { RemoteConnectState } from './remote-connect-state'
import type { VolumeInfo } from '../types'

const log = getAppLogger('servers')

export interface PlaceConnectDeps {
  /** The pane's volume id. */
  getVolumeId: () => string
  /** The pane's live `VolumeInfo` (or null). Owned by the component. */
  getCurrentVolumeInfo: () => VolumeInfo | null
  /**
   * The place is live now. The pane re-runs its listing, which is what turns the
   * connecting view back into a directory.
   */
  onConnected: (volumeId: string) => void
}

export interface PlaceConnect {
  /** What the pane renders instead of a listing, or `null` for a normal pane. */
  readonly state: RemoteConnectState | null
}

export function createPlaceConnect(deps: PlaceConnectDeps): PlaceConnect {
  let state = $state<RemoteConnectState | null>(null)
  /** The attempt a Cancel aims at. Plain, not `$state`: nothing renders it. */
  let attemptId: string | null = null
  /** The volume this factory has already dialed, so landing doesn't loop. */
  let dialed: string | null = null

  $effect(() => {
    const info = deps.getCurrentVolumeInfo()
    const volumeId = deps.getVolumeId()
    if (info?.connectionState !== 'saved') {
      // Live, gone, or a local volume: the pane shows its own listing again.
      state = null
      attemptId = null
      dialed = null
      return
    }
    if (dialed === volumeId) return
    dialed = volumeId
    void dial(volumeId, info)
  })

  async function dial(volumeId: string, info: VolumeInfo) {
    state = {
      kind: 'connecting',
      cancel: () => {
        if (attemptId) void cancelPlaceConnect(attemptId)
      },
    }
    const result = await connectPlace({
      volumeId,
      connectionState: info.connectionState,
      onAttemptStarted: (id) => {
        attemptId = id
      },
      // The sheet, for the moment the backend says a person is what's missing.
      // It owns the rounds from there and stays open across them.
      openSignIn: openSignInForPlace,
    })
    attemptId = null
    switch (result.kind) {
      case 'connected':
      case 'already_live':
        // The row flips to `direct` on the next `volumes-changed`; reloading now
        // is what makes the pane feel like it opened rather than waited.
        state = null
        deps.onConnected(volumeId)
        return
      case 'reconnecting':
        // The backoff loop owns it; its own view takes over once the row moves
        // to `disconnected`. Keep the spinner until it does.
        return
      case 'cancelled':
        // ❗ Says nothing: the user pressed the button. The pane leaves the
        // place through the switcher, the same way it arrived.
        state = null
        return
      case 'refused':
        state = refusedState(volumeId, info, result.refusal)
        return
    }
  }

  function refusedState(volumeId: string, info: VolumeInfo, refusal: ConnectRefusalKind): RemoteConnectState {
    const parsed = parseServerPath(info.path)
    if (!parsed) log.warn('A place refused a dial but its path names no server: {path}', { path: info.path })
    return {
      kind: 'refused',
      refusal: wordConnectRefusal(refusal, {
        host: parsed?.host ?? info.name,
        username: parsed?.username ?? info.name,
      }),
      retry: () => {
        // A fresh attempt, with a fresh id: the old one is spent.
        void dial(volumeId, info)
      },
    }
  }

  return {
    get state() {
      return state
    },
  }
}
