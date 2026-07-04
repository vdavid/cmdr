/**
 * SMB reconnect + direct-upgrade view state for a file pane. Owns the reactive
 * derivations that pick the SMB alt-views (reconnecting spinner / gave-up banner
 * / sign-in prompt), the reconnect-manager subscription `$effect`, and the
 * handlers behind those views (cancel / disconnect) plus the inline
 * "Connect directly" credential-upgrade flow.
 *
 * Lifted out of `FilePane.svelte` into a `*.svelte.ts` factory owning its
 * `$effect` (created synchronously during component init, the
 * `initListingDiffSync` pattern). The pane keeps the shared `currentVolumeInfo`
 * derived (tint + disk-image + eject read it too) and passes it in; the SMB
 * decision deriveds and handlers live here.
 */

import { getIpcErrorMessage } from '$lib/tauri-commands/ipc-types'
import { disconnectSmbVolume, upgradeToSmbVolumeWithCredentials, type UpgradeResult } from '$lib/tauri-commands'
import { smbReconnectManager } from '../network/smb-reconnect-manager.svelte'
import { resolveValidPath } from '../navigation/path-resolution'
import { requestVolumeRefresh } from '$lib/stores/volume-store.svelte'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { getAppLogger } from '$lib/logging/logger'
import type { VolumeInfo } from '../types'

const log = getAppLogger('fileExplorer')

/** Props for the inline SMB "Connect directly" login form (null when hidden). */
export interface SmbUpgradeLoginState {
  volumeId: string
  server: string
  share: string
  port: number
  displayName: string
  usernameHint: string | null
  errorMessage?: string
  isConnecting: boolean
}

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
  /** Props for the inline "Connect directly" login form, or null when hidden. */
  readonly smbUpgradeLogin: SmbUpgradeLoginState | null
  /** Reconnect cycle state for this pane's volume, or null when no cycle is running. */
  readonly reconnectState: ReturnType<typeof smbReconnectManager.getState>
  /** Show the reconnecting spinner (cycle waiting / attempting). */
  readonly showSmbReconnecting: boolean
  /** Show the gave-up banner (cycle exhausted its attempts). */
  readonly showSmbGaveUp: boolean
  /** Show the sign-in prompt (reconnect gave up because the saved password went stale). */
  readonly showSmbNeedsAuth: boolean
  /** Cancel the reconnect cycle and walk up to the nearest reachable folder. */
  handleSmbReconnectCancel: () => void
  /** Cancel the cycle, OS-unmount the share, and navigate away immediately. */
  handleSmbReconnectDisconnect: () => void
  /** Open the inline login form for a "Connect directly" upgrade that needs credentials. */
  handleSmbUpgradeLogin: (info: UpgradeResult & { status: 'credentialsNeeded' }, vid: string) => void
  /** Submit the inline login form: upgrade the OS-mount share to a direct connection. */
  handleSmbUpgradeConnect: (
    username: string | null,
    password: string | null,
    rememberInKeychain: boolean,
  ) => Promise<void>
  /** Dismiss the inline login form. */
  handleSmbUpgradeCancel: () => void
}

export function createSmbViewState(deps: SmbViewStateDeps): SmbViewState {
  let smbUpgradeLogin = $state<SmbUpgradeLoginState | null>(null)

  /** True if this pane is on an SMB share (any state: direct, os_mount, or disconnected). */
  const isSmbVolume = $derived(deps.getCurrentVolumeInfo()?.smbConnectionState != null)
  /**
   * The per-volume reconnect cycle state, or null if no cycle is running. The
   * manager is the single source of truth for the view. By the time this is
   * non-null, the backend has already emitted `disconnected` and the manager has
   * scheduled the first attempt.
   */
  const reconnectState = $derived(smbReconnectManager.getState(deps.getVolumeId()))
  const showSmbReconnecting = $derived(
    reconnectState !== null && (reconnectState.status === 'waiting' || reconnectState.status === 'attempting'),
  )
  const showSmbGaveUp = $derived(reconnectState !== null && reconnectState.status === 'gave-up')
  const showSmbNeedsAuth = $derived(reconnectState !== null && reconnectState.status === 'needs-auth')

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
    const isDisconnected = deps.getCurrentVolumeInfo()?.smbConnectionState === 'disconnected'
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
      const message = getIpcErrorMessage(e)
      log.warn('Disconnect SMB volume {volumeId} failed: {error}', { volumeId: targetVolumeId, error: message })
      addToast(tString('fileExplorer.pane.disconnectFailedToast', { message }), { level: 'error' })
    })
    void resolveValidPath(deps.getCurrentPath(), { volumeRoot: deps.getVolumePath() }).then((validPath) => {
      deps.navigateToFallback(validPath)
    })
  }

  function handleSmbUpgradeLogin(info: UpgradeResult & { status: 'credentialsNeeded' }, vid: string): void {
    smbUpgradeLogin = {
      volumeId: vid,
      server: info.server,
      share: info.share,
      port: info.port,
      displayName: info.displayName,
      usernameHint: info.usernameHint,
      errorMessage: info.message ?? undefined,
      isConnecting: false,
    }
  }

  async function handleSmbUpgradeConnect(
    username: string | null,
    password: string | null,
    rememberInKeychain: boolean,
  ): Promise<void> {
    if (!smbUpgradeLogin) return
    smbUpgradeLogin = { ...smbUpgradeLogin, isConnecting: true, errorMessage: undefined }

    try {
      const result = await upgradeToSmbVolumeWithCredentials(
        smbUpgradeLogin.volumeId,
        username,
        password,
        rememberInKeychain,
      )
      if (result.status === 'success') {
        smbUpgradeLogin = null
        requestVolumeRefresh()
        addToast(tString('fileExplorer.pane.connectedDirectlyToast'), { level: 'success' })
      } else if (result.status === 'credentialsNeeded') {
        smbUpgradeLogin = {
          ...smbUpgradeLogin,
          isConnecting: false,
          errorMessage: result.message ?? tString('fileExplorer.network.authFailed'),
        }
      } else {
        smbUpgradeLogin = null
        addToast(tString('fileExplorer.pane.directConnectionFailedToast', { message: result.message }), {
          level: 'error',
        })
      }
    } catch (e) {
      smbUpgradeLogin = null
      addToast(tString('fileExplorer.pane.directConnectionFailedToast', { message: String(e) }), {
        level: 'error',
      })
    }
  }

  function handleSmbUpgradeCancel(): void {
    smbUpgradeLogin = null
  }

  return {
    get smbUpgradeLogin() {
      return smbUpgradeLogin
    },
    get reconnectState() {
      return reconnectState
    },
    get showSmbReconnecting() {
      return showSmbReconnecting
    },
    get showSmbGaveUp() {
      return showSmbGaveUp
    },
    get showSmbNeedsAuth() {
      return showSmbNeedsAuth
    },
    handleSmbReconnectCancel,
    handleSmbReconnectDisconnect,
    handleSmbUpgradeLogin,
    handleSmbUpgradeConnect,
    handleSmbUpgradeCancel,
  }
}
