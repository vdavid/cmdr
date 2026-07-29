/**
 * The Network host a pane currently has open, plus any share queued to
 * auto-mount on it. Kept on the pane (not inside `NetworkMountView`) because the
 * view re-mounts, and because history navigation tracks the host.
 *
 * Guardrail: leaving the network volume by ANY route clears both, so re-entering
 * Network always lands on the host list. Clearing only on `ShareBrowser`'s Back
 * button (the obvious place) misses volume switches from the picker, the
 * breadcrumb, history navigation, and MCP, which leaves `NetworkMountView`
 * re-mounting with a stale `initialNetworkHost` and the user staring at the
 * previous host's share list. See `file-explorer/network/CLAUDE.md`.
 */

import type { NetworkHost } from '../types'

export interface NetworkHostStateDeps {
  getIsNetworkView: () => boolean
  /** Bubble a host change out of the pane (history tracking). */
  onHostChange: (host: NetworkHost | null) => void
}

export interface NetworkHostState {
  /** The host whose shares the pane is showing, or null for the host list. */
  readonly host: NetworkHost | null
  /** A share to mount as soon as the share browser is ready, or undefined. */
  readonly autoMountShare: string | undefined
  /** Set the host without bubbling (the pane API's `setNetworkHost`). */
  setHost: (host: NetworkHost | null) => void
  /** The view reports a host change: remember it AND bubble it. */
  handleHostChange: (host: NetworkHost | null) => void
  setAutoMountShare: (shareName: string | undefined) => void
}

export function createNetworkHostState(deps: NetworkHostStateDeps): NetworkHostState {
  let host = $state<NetworkHost | null>(null)
  let autoMountShare = $state<string | undefined>(undefined)

  $effect(() => {
    if (deps.getIsNetworkView()) return
    if (host !== null) host = null
    if (autoMountShare !== undefined) autoMountShare = undefined
  })

  return {
    get host() {
      return host
    },
    get autoMountShare() {
      return autoMountShare
    },
    setHost: (next) => {
      host = next
    },
    handleHostChange: (next) => {
      host = next
      deps.onHostChange(next)
    },
    setAutoMountShare: (shareName) => {
      autoMountShare = shareName
    },
  }
}
