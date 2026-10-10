/**
 * A pane standing on a saved place, and the dial that brings it to life.
 *
 * ❗ **Opening a place that isn't live brings it to life IN THE PANE, with a
 * cancel** (`$lib/servers/DETAILS.md` § "The four rules", rule 2). MTP already
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
import { wordPaneRefusal } from '$lib/servers/connect-refusals'
import { isAtOrUnder, parseServerPath } from '$lib/servers/server-path-utils'
import { volumeScheme } from '$lib/volume-scheme'
import { resolveValidPath } from '../navigation/path-resolution'
import { getAppLogger } from '$lib/logging/logger'
import type { RemoteConnectState } from './remote-connect-state'
import type { VolumeInfo } from '../types'
import type { VolumeChangePayload } from './types'
import { isLiveSession } from '../navigation/connection-state'

const log = getAppLogger('servers')

export interface PlaceConnectDeps {
  /** The pane's volume id. */
  getVolumeId: () => string
  /** The pane's live `VolumeInfo` (or null). Owned by the component. */
  getCurrentVolumeInfo: () => VolumeInfo | null
  /** The root the pane holds for its volume (the `volumePath` it was entered with). */
  getVolumePath: () => string
  /** The folder the pane stands in. */
  getCurrentPath: () => string
  /**
   * The folder the pane was last sent to, committed TOGETHER with `getVolumeId()`. ❗ The
   * follow below reads this, ❌ never `getCurrentPath()`: that one catches up an effect
   * after a volume switch, so for one run it still names the previous volume's folder.
   */
  getEnteredPath: () => string
  /**
   * Enters the volume: the route a switcher pick takes, so the pane's root, its
   * path, its listing, and its disk space all move together. Used once the place
   * is live, and whenever a live share's mount path isn't the root the pane holds.
   */
  enter: (change: VolumeChangePayload) => void
  /**
   * Where a place sits once live, off the saved list. Asked only for an SMB
   * share, whose next mount can land on another path (`/Volumes/naspi-1`) than
   * the one its saved row remembered; a server place's root never moves on a
   * connect.
   */
  landingOf?: (volumeId: string) => Promise<string | null>
  /** Back to where the pane was before the place, for the view a cancel leaves. */
  goBack?: () => void
  /** Whether there is a back to go to; without one the view offers none. */
  canGoBack?: () => boolean
}

export interface PlaceConnect {
  /** What the pane renders instead of a listing, or `null` for a normal pane. */
  readonly state: RemoteConnectState | null
  /**
   * The person picked `volumeId` (the switcher, the hub). ❗ A pick of the place the
   * pane already stands on, not connected, dials it like Try again: it is the
   * pane's own volume, so nothing else moves, and it used to do nothing at all.
   */
  picked: (volumeId: string) => void
}

export function createPlaceConnect(deps: PlaceConnectDeps): PlaceConnect {
  let state = $state<RemoteConnectState | null>(null)
  /** The attempt a Cancel aims at. Plain, not `$state`: nothing renders it. */
  let attemptId: string | null = null
  /** The volume this factory has already dialed, so landing doesn't loop. */
  let dialed: string | null = null
  /** The mount path last followed, so a volume-list refresh before the pane's root catches up doesn't enter twice. */
  let followed: string | null = null

  $effect(() => {
    const info = deps.getCurrentVolumeInfo()
    const volumeId = deps.getVolumeId()
    // ❗ A live share is followed to wherever its mount IS. Its next mount can land
    // on another `/Volumes` path than the one the pane was entered at, and a pane
    // whose root and volume disagree lists one server under another's path, where
    // a write would reach the wrong one (QA round 4, R3-A). The live row's path is
    // `statfs`'s, so it is the one to trust.
    if (info && volumeScheme(volumeId) === 'smb' && isLiveSession(info.connectionState)) {
      const root = deps.getVolumePath()
      const entered = deps.getEnteredPath()
      // ❗ Also when the root matches but the folder is outside the mount: a mount that
      // finished after a Cancel left the pane at the stale saved path, listing "Not
      // connected yet" over a share that was live elsewhere (final QA).
      const outside = !isAtOrUnder(entered, info.path)
      if ((info.path !== root || outside) && followed !== `${volumeId}:${info.path}:${entered}`) {
        followed = `${volumeId}:${info.path}:${entered}`
        log.info('The share {volumeId} is mounted at {path}, not {root} (pane sent to {entered}); following it', {
          volumeId,
          path: info.path,
          root,
          entered,
        })
        deps.enter({
          volumeId,
          volumePath: info.path,
          targetPath: rebaseOnRoot(entered, root, info.path),
        })
      }
    }
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
    // ❗ The pane left this place while the dial was out (a server move followed it to
    // the new address, which the backend answers with `cancelled`): the answer is
    // about a place it no longer shows, so it neither words a view nor enters.
    if (deps.getVolumeId() !== volumeId) return
    attemptId = null
    switch (result.kind) {
      case 'connected':
      case 'already_live': {
        // The row flips to `direct` on the next `volumes-changed`; reloading now
        // is what makes the pane feel like it opened rather than waited.
        const landing = await landingOf(volumeId)
        const root = deps.getVolumePath()
        const volumePath = landing ?? root
        const targetPath = await folderThatExists(
          volumeId,
          rebaseOnRoot(deps.getCurrentPath(), root, volumePath),
          volumePath,
        )
        state = null
        deps.enter({ volumeId, volumePath, targetPath })
        return
      }
      case 'reconnecting':
        // The backoff loop owns it; its own view takes over once the row moves
        // to `disconnected`. Keep the spinner until it does.
        return
      case 'cancelled':
        // ❗ A kernel mount can finish after the Cancel: then the place is live, the
        // `$effect` has already followed it, and there's nothing "not connected" to say.
        if (isLiveSession(deps.getCurrentVolumeInfo()?.connectionState)) {
          state = null
          return
        }
        // ❗ Says nothing about WHY (the user pressed the button), but stays on the
        // place: its name, a way to connect again, and the way back.
        state = {
          kind: 'not_connected',
          connect: () => {
            void dial(volumeId, deps.getCurrentVolumeInfo() ?? info)
          },
          goBack: deps.goBack && deps.canGoBack?.() !== false ? deps.goBack : null,
        }
        return
      case 'refused':
        state = refusedState(volumeId, info, result.refusal, result.region)
        return
    }
  }

  /**
   * Where a place that just went live sits: ❗ where it is LIVE first, since a mount
   * that finished after a Cancel never updated the saved row, whose remembered path
   * then read "Not connected yet" on Try again. Only an SMB share moves; `null` keeps
   * the pane's root.
   */
  async function landingOf(volumeId: string): Promise<string | null> {
    if (volumeScheme(volumeId) !== 'smb') return null
    const live = deps.getCurrentVolumeInfo()
    if (live && isLiveSession(live.connectionState)) return live.path
    return (await deps.landingOf?.(volumeId)) ?? null
  }

  /**
   * The deepest folder of `target` that exists inside a place that just went live,
   * else its root. ❗ A restored tab (`initialization.ts`) or a favorite keeps the
   * folder it points at inside an unconnected share or server, and that folder may be
   * gone by the time the session is up. The walk asks the place itself (`volumeId`),
   * which is live by now, and stops at its root.
   */
  async function folderThatExists(volumeId: string, target: string, volumePath: string): Promise<string> {
    if (target === volumePath || !walksOnConnect(volumeId)) return target
    const found = await resolveValidPath(target, { volumeRoot: volumePath, volumeId })
    return found && isAtOrUnder(found, volumePath) ? found : volumePath
  }

  function refusedState(
    volumeId: string,
    info: VolumeInfo,
    refusal: ConnectRefusalKind,
    region: string | undefined,
  ): RemoteConnectState {
    const parsed = parseServerPath(info.path)
    // An SMB share's path is its mount point, which names no server by design.
    if (!parsed && volumeScheme(volumeId) !== 'smb') {
      log.warn('A place refused a dial but its path names no server: {path}', { path: info.path })
    }
    return {
      kind: 'refused',
      refusal: wordPaneRefusal(refusal, {
        host: parsed?.host ?? info.name,
        username: parsed?.username ?? info.name,
        name: info.name,
        protocol: parsed?.protocol,
        region,
      }),
      retry: () => {
        // A fresh attempt, with a fresh id: the old one is spent.
        //
        // ❗ And a fresh reading of the volume's STANDING, ❌ never the one
        // captured when the refusal landed. `connectPlace` picks its arm by that
        // standing, and getting it wrong is silent: if a concurrent connect from
        // the hub or the switcher registered the volume in between, a stale
        // `saved` would send this down the dial arm and register a SECOND volume
        // under a second id.
        void dial(volumeId, deps.getCurrentVolumeInfo() ?? info)
      },
    }
  }

  function picked(volumeId: string): void {
    if (volumeId !== deps.getVolumeId()) return
    if (state?.kind !== 'not_connected' && state?.kind !== 'refused') return
    const info = deps.getCurrentVolumeInfo()
    if (info) void dial(volumeId, info)
  }

  return {
    get state() {
      return state
    },
    picked,
  }
}

/**
 * Whether a place this factory dials is one whose folders it checks once live: a share
 * or a server. A phone dials through `device-connect.svelte.ts` instead, and the rest
 * never reach a `saved` row.
 */
function walksOnConnect(volumeId: string): boolean {
  const scheme = volumeScheme(volumeId)
  switch (scheme) {
    case 'smb':
    case 'sftp':
    case 'webdav':
    case 's3':
      return true
    case 'root':
    case 'local':
    case 'path':
    case 'mtp':
    case 'adb':
    case 'cloud':
    case 'favorite':
    case 'unknown':
      return false
  }
}

/**
 * `path` moved from under `oldRoot` to under `newRoot`, keeping the folder inside:
 * `/Volumes/naspi/docs` is `/Volumes/naspi-1/docs` once the share mounts there. A
 * path outside `oldRoot` lands at `newRoot`.
 */
export function rebaseOnRoot(path: string, oldRoot: string, newRoot: string): string {
  if (path === oldRoot) return newRoot
  const base = oldRoot.endsWith('/') ? oldRoot : `${oldRoot}/`
  if (!path.startsWith(base)) return newRoot
  return `${newRoot.endsWith('/') ? newRoot : `${newRoot}/`}${path.slice(base.length)}`
}
