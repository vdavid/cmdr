/**
 * SMB reconnect view state for a file pane. Turns the reconnect manager's status
 * into the one `RemoteConnectState` the pane renders (plus the gave-up banner's
 * own flag), owns the manager subscription `$effect`, and holds the handlers
 * behind those views (retry now / cancel / disconnect / sign in).
 *
 * Lifted out of `FilePane.svelte` into a `*.svelte.ts` factory owning its
 * `$effect` (created synchronously during component init, the
 * `initListingDiffSync` pattern). The pane keeps the shared `currentVolumeInfo`
 * derived (tint + disk-image + eject read it too) and passes it in; the SMB
 * decision deriveds and handlers live here.
 *
 * ❗ The credential ask is ❌ NOT here: it is the one app-global sign-in sheet,
 * raised by `network/direct-connect.ts` and `servers/open-sign-in.ts`. A pane
 * that hosted a form was what made "which pane can render it right now" a
 * question at all.
 */

import { wordEjectRefusal } from '../navigation/eject-error-messages'
import { disconnectPlace, disconnectSmbVolume } from '$lib/tauri-commands'
import { openSignInForPlace } from '$lib/servers/open-sign-in'
import { reconnectCycleLines, smbReconnectManager } from '../network/smb-reconnect-manager.svelte'
import { resolveValidPath } from '../navigation/path-resolution'
import { hasReconnectLoop } from '../navigation/connection-state'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { getAppLogger } from '$lib/logging/logger'
import type { VolumeInfo } from '../types'
import type { RemoteConnectState } from './remote-connect-state'

const log = getAppLogger('fileExplorer')

export interface SmbViewStateDeps {
  getVolumeId: () => string
  getCurrentPath: () => string
  getVolumePath: () => string
  /** The pane's live `VolumeInfo` (or null). Owned by the component; shared with tint/eject. */
  getCurrentVolumeInfo: () => VolumeInfo | null
  /** Reload the current directory after a successful reconnect. */
  loadDirectory: (path: string) => void
  /** Walk up to the nearest reachable folder (or switch to root) after leaving a broken share. */
  navigateToFallback: (validPath: string | null) => void
}

export interface SmbViewState {
  /**
   * What `RemoteConnectView` should render for this pane, or `null` when the
   * pane shows its listing.
   *
   * ❗ ONE exhaustive `switch` over the manager's status, so a new status can't
   * quietly render a plain listing over a dead session — which is what a list of
   * per-status booleans allowed.
   */
  readonly remoteConnectState: RemoteConnectState | null
  /**
   * Show the gave-up banner (the cycle ran out of attempts).
   *
   * ❗ Its own flag rather than a `RemoteConnectState` variant:
   * `VolumeUnreachableBanner` is the app's one "couldn't reach this" surface,
   * and two renderers for one state is worse than one in the file next door.
   */
  readonly showGaveUp: boolean
  /** Try now: skip the running cycle's wait, or restart one that gave up. */
  handleRetryNow: () => void
  /** Open the sign-in sheet for this pane's place, from the signed-out banner. */
  handleSignIn: () => void
  /** Drop a server place's dead session and leave, so the next open dials afresh. */
  handleDisconnectPlace: () => void
  /** Cancel the reconnect cycle and walk up to the nearest reachable folder. */
  handleSmbReconnectCancel: () => void
  /** Cancel the cycle, OS-unmount the share, and navigate away immediately. */
  handleSmbReconnectDisconnect: () => void
}

export function createSmbViewState(deps: SmbViewStateDeps): SmbViewState {
  /**
   * True when the per-volume reconnect manager owns this pane's volume, so the
   * pane subscribes. ❗ Not a `!= null` test on the state any more: a saved server
   * that was never connected carries one and has no cycle to join, and enrolling
   * it would start a backoff loop against a server nobody asked to dial.
   */
  const isSmbVolume = $derived(hasReconnectLoop(deps.getCurrentVolumeInfo()?.connectionState))
  /**
   * The per-volume reconnect cycle state, or null if no cycle is running. The
   * manager is the single source of truth for the view. By the time this is
   * non-null, the backend has already emitted `disconnected` and the manager has
   * scheduled the first attempt.
   */
  const reconnectState = $derived(smbReconnectManager.getState(deps.getVolumeId()))

  /**
   * The manager's status, as the pane renders it. ❗ Exhaustive: `gave-up` is the
   * one status with no pane view of its own (`showGaveUp` sends it to
   * `VolumeUnreachableBanner`), and every other status names what it shows.
   */
  const remoteConnectState = $derived.by<RemoteConnectState | null>(() => {
    const cycle = reconnectState
    if (!cycle) return null
    switch (cycle.status) {
      case 'waiting':
      case 'attempting':
        return {
          kind: 'connecting',
          cancel: handleSmbReconnectCancel,
          cycle: {
            lines: reconnectCycleLines(cycle.attemptIndex),
            waiting:
              cycle.status === 'waiting' ? { startedAt: cycle.waitStartedAt, durationMs: cycle.currentDelayMs } : null,
            retryNow: handleRetryNow,
            disconnect: handleSmbReconnectDisconnect,
          },
        }
      case 'needs-auth':
        // ❗ `nothing` means no secret a person could type would help, so the
        // banner says what happened and offers no button.
        return {
          kind: 'signed_out',
          signIn: smbReconnectManager.getSignInShape(deps.getVolumeId())?.kind === 'nothing' ? null : handleSignIn,
        }
      case 'needs-host-key':
        return { kind: 'host_key_changed', disconnect: handleDisconnectPlace }
      case 'gave-up':
        return null
    }
  })
  const showGaveUp = $derived(reconnectState !== null && reconnectState.status === 'gave-up')

  // Subscribe to the per-volume reconnect manager whenever this pane is on an SMB
  // share. The subscription is refcounted (multiple panes on the same share share
  // one cycle) and serves two purposes:
  // 1. Tells the manager "someone is watching": the cycle starts on the next
  //    `disconnected` event (via `handleDisconnected`), but only if subscribers > 0.
  // 2. Registers a success callback so the pane re-runs `loadDirectory` after a
  //    successful reconnect. (The reactive deriveds cover showing/hiding the view.)
  $effect(() => {
    if (!isSmbVolume) return
    const targetVolumeId = deps.getVolumeId()
    const isDisconnected = deps.getCurrentVolumeInfo()?.connectionState === 'disconnected'
    const onSuccess = () => {
      const path = deps.getCurrentPath()
      log.info('[FilePane] SMB reconnect succeeded for {volumeId}, reloading {path}', {
        volumeId: targetVolumeId,
        path,
      })
      deps.loadDirectory(path)
    }
    const unsubscribe = smbReconnectManager.subscribe(targetVolumeId, onSuccess)
    // If we land on a Disconnected SMB share without a cycle running (e.g. user
    // navigated to a share that was already broken), kick off the cycle ourselves.
    if (isDisconnected) {
      smbReconnectManager.startCycle(targetVolumeId)
    }
    return unsubscribe
  })

  /**
   * The signed-out banner's button. ❗ `registered: true`: a volume is filed
   * under this id, so the sheet MENDS it with `reconnectVolumeWithCredentials`
   * rather than dialing, which would register a second volume under a second id.
   */
  function handleSignIn(): void {
    void openSignInForPlace({ volumeId: deps.getVolumeId(), registered: true })
  }

  /**
   * The changed-key banner's button: drop the dead session and leave.
   *
   * ❗ This is also the way OUT of a changed key. Nothing here holds the
   * fingerprint (the backend keeps no pending prompt for a registered volume),
   * and after this the place is a `saved` row again, so opening it dials afresh
   * and the dial's host-key outcome is what the sheet's key step renders.
   */
  function handleDisconnectPlace(): void {
    const targetVolumeId = deps.getVolumeId()
    smbReconnectManager.cancel(targetVolumeId)
    void disconnectPlace(targetVolumeId).catch((e: unknown) => {
      log.warn('Disconnecting the place {volumeId} broke down: {error}', { volumeId: targetVolumeId, error: String(e) })
    })
    void resolveValidPath(deps.getCurrentPath(), { volumeRoot: deps.getVolumePath() }).then((validPath) => {
      deps.navigateToFallback(validPath)
    })
  }

  /** Skips the current wait and attempts now. Also the gave-up banner's Retry. */
  function handleRetryNow(): void {
    smbReconnectManager.retryNow(deps.getVolumeId())
  }

  function handleSmbReconnectCancel(): void {
    smbReconnectManager.cancel(deps.getVolumeId())
    // Walk up to the nearest reachable folder, same fallback chain we use elsewhere.
    void resolveValidPath(deps.getCurrentPath(), { volumeRoot: deps.getVolumePath() }).then((validPath) => {
      deps.navigateToFallback(validPath)
    })
  }

  function handleSmbReconnectDisconnect(): void {
    const targetVolumeId = deps.getVolumeId()
    smbReconnectManager.cancel(targetVolumeId)
    // Fire the OS-level unmount (macOS: `diskutil unmount`). We don't await here.
    // The FSEvents-driven `volumes-changed` will tear down the SmbVolume and
    // remove the entry; meanwhile the user expects the pane to leave the broken
    // share immediately, so navigate away in parallel.
    void disconnectSmbVolume(targetVolumeId).catch((e: unknown) => {
      // `wordEjectRefusal` logs the backend's technical detail and hands back
      // the localized sentence; the raw `diskutil` text never reaches the toast.
      addToast(tString('fileExplorer.pane.disconnectFailedToast', { message: wordEjectRefusal(e) }), {
        level: 'error',
      })
    })
    void resolveValidPath(deps.getCurrentPath(), { volumeRoot: deps.getVolumePath() }).then((validPath) => {
      deps.navigateToFallback(validPath)
    })
  }

  return {
    get remoteConnectState() {
      return remoteConnectState
    },
    get showGaveUp() {
      return showGaveUp
    },
    handleRetryNow,
    handleSignIn,
    handleDisconnectPlace,
    handleSmbReconnectCancel,
    handleSmbReconnectDisconnect,
  }
}
