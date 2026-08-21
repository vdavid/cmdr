/**
 * The "Connect directly" flow: turn an OS-mounted SMB share into a direct smb2
 * session, with all the feedback that goes with it.
 *
 * ONE implementation, three entry points: the yellow-dot popup in
 * `VolumeBreadcrumb`, the same item in the breadcrumb dropdown's submenu, and the
 * retry button on the OS-mount fallback notice
 * (`SmbOsMountFallbackToastContent`). A second copy would be a second place for
 * the saved-password probe, the toast lifecycle, and the credential fallback to
 * drift.
 *
 * The credential form is the one piece the flow can't own: it renders inside a
 * file pane. Callers hand in `raiseCredentialsForm`; the notice resolves one from
 * `smb-login-hosts`.
 */

import {
  upgradeToSmbVolume,
  upgradeToSmbVolumeUsingSavedPassword,
  systemHasSavedSmbPassword,
  type UpgradeResult,
} from '$lib/tauri-commands'
import { ask } from '@tauri-apps/plugin-dialog'
import { addToast, dismissToast } from '$lib/ui/toast'
import { requestVolumeRefresh } from '$lib/stores/volume-store.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { tString } from '$lib/intl/messages.svelte'
import { triggerNetworkDiscovery } from './lazy-trigger'
import { directConnectionUnavailableMessage } from './upgrade-messages'

const log = getAppLogger('fileExplorer')

/** The `credentialsNeeded` arm of an upgrade result, which is all the form needs. */
export type CredentialsNeeded = UpgradeResult & { status: 'credentialsNeeded' }

/**
 * Raises the inline credential form for `volumeId`. Returns `false` when nothing
 * could show it, which is what stops a click from looking like it did nothing:
 * the flow then says so out loud instead.
 */
export type RaiseCredentialsForm = (info: CredentialsNeeded, volumeId: string) => boolean

/** Where the flow left the volume once it ran its course. */
export type DirectConnectOutcome =
  /** A direct smb2 session is installed. Any notice about the slow path is stale. */
  | 'connected'
  /** The user is being asked for credentials; the answer arrives through the form. */
  | 'askingForCredentials'
  /** No direct session, and the user has been told why. The share still works. */
  | 'stillOnOsMount'

/**
 * Upgrades `volumeId` to a direct smb2 connection, owning every toast along the
 * way (a persistent "Connecting directly…" while it runs, then success or the
 * typed reason it didn't happen).
 *
 * Stored credentials are tried first; if they're missing or stale, the saved
 * macOS/Finder password gets a prompt-free probe before anyone is asked to type
 * anything.
 *
 * Never resolves without having said something to the user, so a caller can wire
 * a button straight to it.
 */
export async function connectDirectly(
  volumeId: string,
  raiseCredentialsForm: RaiseCredentialsForm,
): Promise<DirectConnectOutcome> {
  // Opening a TCP socket to a private IP triggers macOS's Local Network prompt on
  // its own, so this is the right moment to also start mDNS for the rest of the
  // network UI.
  triggerNetworkDiscovery()

  const connectingToastId = addToast(tString('fileExplorer.navigation.connectingDirectly'), {
    dismissal: 'persistent',
  })

  try {
    const result = await upgradeToSmbVolume(volumeId)
    dismissToast(connectingToastId)

    if (result.status === 'success') return announceSuccess()
    if (result.status === 'credentialsNeeded') {
      // Before asking anyone to type a password, see whether macOS/Finder already
      // saved one for this share (a prompt-free probe).
      const saved = await tryUseSavedPassword(volumeId, result.displayName, raiseCredentialsForm)
      return saved ?? askForCredentials(result, volumeId, raiseCredentialsForm)
    }
    addToast(directConnectionUnavailableMessage(result.reason, result.displayName), { level: 'error' })
    return 'stillOnOsMount'
  } catch (e) {
    dismissToast(connectingToastId)
    return announceBreakdown(e)
  }
}

/**
 * If macOS/Finder already saved a password for this share, offer to reuse it so
 * the user doesn't retype it. A prompt-free probe decides whether to offer; on
 * "Use saved password" we prime the user (the macOS Keychain consent dialog comes
 * next, and we can't customize its text) then read and connect.
 *
 * Returns the settled outcome when it fully handled the connection, or `null`
 * when there's nothing saved or the user chose to type it instead, in which case
 * the caller raises the login form.
 */
async function tryUseSavedPassword(
  volumeId: string,
  displayName: string,
  raiseCredentialsForm: RaiseCredentialsForm,
): Promise<DirectConnectOutcome | null> {
  if (!(await systemHasSavedSmbPassword(volumeId))) return null

  const useSaved = await ask(tString('fileExplorer.navigation.useSavedPasswordMessage', { displayName }), {
    title: tString('fileExplorer.navigation.useSavedPasswordTitle'),
    kind: 'info',
    okLabel: tString('fileExplorer.navigation.useSavedPasswordConfirm'),
    cancelLabel: tString('fileExplorer.navigation.useSavedPasswordCancel'),
  })
  if (!useSaved) return null

  const savedToastId = addToast(tString('fileExplorer.navigation.connectingWithSavedPassword'), {
    dismissal: 'persistent',
  })
  try {
    const result = await upgradeToSmbVolumeUsingSavedPassword(volumeId)
    dismissToast(savedToastId)
    if (result.status === 'success') return announceSuccess()
    if (result.status === 'credentialsNeeded') {
      // The saved password was absent, denied, or wrong: fall to the login form.
      return askForCredentials(result, volumeId, raiseCredentialsForm)
    }
    addToast(directConnectionUnavailableMessage(result.reason, result.displayName), { level: 'error' })
    return 'stillOnOsMount'
  } catch (e) {
    dismissToast(savedToastId)
    return announceBreakdown(e)
  }
}

function announceSuccess(): DirectConnectOutcome {
  addToast(tString('fileExplorer.pane.connectedDirectlyToast'), { level: 'success' })
  requestVolumeRefresh()
  return 'connected'
}

/**
 * Hands the credential prompt to whoever can render it. When nobody can (every
 * pane is between mounts, or the caller wired no form at all), the share is still
 * on the OS mount and the user has to hear that rather than watch a click vanish.
 */
function askForCredentials(
  info: CredentialsNeeded,
  volumeId: string,
  raiseCredentialsForm: RaiseCredentialsForm,
): DirectConnectOutcome {
  if (raiseCredentialsForm(info, volumeId)) return 'askingForCredentials'
  addToast(tString('fileExplorer.pane.directConnectionUnavailableToast'), { level: 'error' })
  return 'stillOnOsMount'
}

function announceBreakdown(e: unknown): DirectConnectOutcome {
  log.error('Direct SMB connection attempt broke down', { error: String(e) })
  addToast(tString('fileExplorer.pane.directConnectionUnavailableToast'), { level: 'error' })
  return 'stillOnOsMount'
}
