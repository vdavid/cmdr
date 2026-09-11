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
 * The credential ask is the ONE sign-in sheet (`servers/DETAILS.md` § "The sheet
 * contract"), which is app-global, so this flow raises it itself and every caller
 * is a bare `connectDirectly({ volumeId, shareName })`. ❗ It returns as soon as
 * the sheet is up, ❌ not when the user is done with it: the OS-mount notice
 * retires on `askingForCredentials`, and waiting out a sheet would leave it
 * stacked under one.
 */

import {
  upgradeToSmbVolume,
  upgradeToSmbVolumeUsingSavedPassword,
  upgradeToSmbVolumeWithCredentials,
  systemHasSavedSmbPassword,
  type UpgradeResult,
} from '$lib/tauri-commands'
import { ask } from '@tauri-apps/plugin-dialog'
import { addToast, dismissToast } from '$lib/ui/toast'
import { requestVolumeRefresh } from '$lib/stores/volume-store.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { tString } from '$lib/intl/messages.svelte'
import { triggerNetworkDiscovery } from './lazy-trigger'
import { openSmbSignInSheet, type SmbCredentialAnswer } from './smb-sign-in'
import type { SignInAttemptOutcome } from '$lib/servers/sign-in-contract'
import type { ConnectRefusalKind } from '$lib/servers/connect-refusals'
import type { CredentialsNeededReason } from '$lib/ipc/bindings'
import {
  directConnectionUnavailableMessage,
  mountNotRespondingMessage,
  nothingToUpgradeMessage,
} from './upgrade-messages'

const log = getAppLogger('fileExplorer')

/** The `credentialsNeeded` arm of an upgrade result, which is all the form needs. */
export type CredentialsNeeded = UpgradeResult & { status: 'credentialsNeeded' }

/**
 * What the sheet says, for each reason the backend needed a credential.
 *
 * ❗ A `Record`, so a new reason can't reach the sheet without a refusal to say.
 * ❌ `accountNotPermitted` is not `authentication_rejected`: that account SIGNED
 * IN, and calling it a wrong password kept the sheet asking for a password that
 * worked (ERR-SHUSC).
 */
const REFUSAL_FOR_REASON: Record<CredentialsNeededReason, ConnectRefusalKind> = {
  noCredential: 'needs_credentials',
  credentialRejected: 'authentication_rejected',
  accountNotPermitted: 'account_not_permitted',
}

/** The answers that leave no direct session and no credential worth asking for. */
type NoUpgrade = Exclude<UpgradeResult, { status: 'success' | 'credentialsNeeded' }>

/** The volume a "Connect directly" press is about. */
export interface DirectConnectTarget {
  volumeId: string
  /**
   * The name the pressed control showed: the share on the OS-mount notice, the
   * volume in the breadcrumb. It words the "that share is gone" answer, which the
   * backend can't name.
   */
  shareName: string
}

/** Where the flow left the volume once it ran its course. */
export type DirectConnectOutcome =
  /** A direct smb2 session is installed. Any notice about the slow path is stale. */
  | 'connected'
  /** The user is being asked for credentials; the answer arrives through the form. */
  | 'askingForCredentials'
  /** No direct session, and the user has been told why. The share still works. */
  | 'stillOnOsMount'
  /**
   * There's no OS-mounted share left to upgrade: it was unmounted, ejected, or
   * dropped off the network before the press, or the volume was never a network
   * share. The user has been told, and no retry can change it.
   */
  | 'gone'

/**
 * Upgrades the target volume to a direct smb2 connection, owning every toast
 * along the way (a persistent "Connecting directly…" while it runs, then success
 * or the typed reason it didn't happen).
 *
 * Stored credentials are tried first; if they're missing or stale, the saved
 * macOS/Finder password gets a prompt-free probe before anyone is asked to type
 * anything.
 *
 * Never resolves without having said something to the user, so a caller can wire
 * a button straight to it.
 */
export async function connectDirectly(target: DirectConnectTarget): Promise<DirectConnectOutcome> {
  // Opening a TCP socket to a private IP triggers macOS's Local Network prompt on
  // its own, so this is the right moment to also start mDNS for the rest of the
  // network UI.
  triggerNetworkDiscovery()

  const connectingToastId = addToast(tString('fileExplorer.navigation.connectingDirectly'), {
    dismissal: 'persistent',
  })

  try {
    const result = await upgradeToSmbVolume(target.volumeId)
    dismissToast(connectingToastId)

    if (result.status === 'success') return announceSuccess()
    if (result.status === 'credentialsNeeded') {
      // Before asking anyone to type a password, see whether macOS/Finder already
      // saved one for this share (a prompt-free probe).
      const saved = await tryUseSavedPassword(target, result.displayName)
      return saved ?? askForCredentials(result, target)
    }
    return announceNoUpgrade(result, target.shareName)
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
  target: DirectConnectTarget,
  displayName: string,
): Promise<DirectConnectOutcome | null> {
  if (!(await systemHasSavedSmbPassword(target.volumeId))) return null

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
    const result = await upgradeToSmbVolumeUsingSavedPassword(target.volumeId)
    dismissToast(savedToastId)
    if (result.status === 'success') return announceSuccess()
    if (result.status === 'credentialsNeeded') {
      // The saved password was absent, denied, or wrong: fall to the sheet.
      return askForCredentials(result, target)
    }
    return announceNoUpgrade(result, target.shareName)
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
 * Opens the one sign-in sheet on this share, and reports that the ask is up.
 *
 * ❗ The sheet's promise is deliberately NOT awaited: the answer arrives on its
 * own schedule, and the notice that pressed this button retires the moment the
 * ask is on screen (two prompts for one share is noise). The sheet is app-global,
 * so there is no longer a case where nothing can render it.
 */
function askForCredentials(info: CredentialsNeeded, target: DirectConnectTarget): DirectConnectOutcome {
  void openSmbSignInSheet({
    host: { name: info.displayName },
    shareName: info.share,
    // ❗ No guest option: connecting with no credential at all is exactly what
    // `upgradeToSmbVolume` just tried, so offering it again would be inert.
    guestAllowed: false,
    refusal: REFUSAL_FOR_REASON[info.reason],
    // A refusal names the account it turned away, so that account opens the sheet.
    // With nothing offered, the store's remembered username is the better guess.
    initialUsername: info.reason === 'noCredential' ? undefined : (info.usernameHint ?? undefined),
    attempt: (answer) => upgradeWithCredentials(target, answer),
  })
  return 'askingForCredentials'
}

/**
 * One upgrade round-trip with what the user offered.
 *
 * ❗ Only an answer another credential can fix keeps the sheet open, saying
 * which: a password the server refused, or an account the share doesn't let in.
 * A server that stopped answering, or a share that went away while the user
 * typed, has nothing a credential can fix, so the sheet closes and the typed
 * sentence goes to a toast: the same words the flow's own paths use.
 */
async function upgradeWithCredentials(
  target: DirectConnectTarget,
  answer: SmbCredentialAnswer,
): Promise<SignInAttemptOutcome> {
  try {
    const result = await upgradeToSmbVolumeWithCredentials(
      target.volumeId,
      answer.username,
      answer.password,
      answer.remember,
    )
    if (result.status === 'success') {
      announceSuccess()
      return { kind: 'handed_off' }
    }
    if (result.status === 'credentialsNeeded') return { kind: 'refused', refusal: REFUSAL_FOR_REASON[result.reason] }
    announceNoUpgrade(result, target.shareName)
    return { kind: 'handed_off' }
  } catch (e) {
    announceBreakdown(e)
    return { kind: 'handed_off' }
  }
}

/**
 * Says why no direct session came of it, and where that leaves the volume.
 *
 * A server that didn't cooperate, or a mount that stopped answering, is worth
 * another press once it wakes, so that's an `error` and the volume is
 * `stillOnOsMount`. A share that's gone, or a volume that was never one, isn't
 * worth any press: nothing broke, there was just nothing left to connect, so
 * that's a `warn` and the volume is `gone`.
 */
function announceNoUpgrade(result: NoUpgrade, shareName: string): DirectConnectOutcome {
  if (result.status === 'networkError') {
    addToast(directConnectionUnavailableMessage(result.reason, result.displayName), { level: 'error' })
    return 'stillOnOsMount'
  }
  if (result.status === 'mountNotResponding') {
    addToast(mountNotRespondingMessage(shareName), { level: 'error' })
    return 'stillOnOsMount'
  }
  addToast(nothingToUpgradeMessage(result.status, shareName), { level: 'warn' })
  return 'gone'
}

/** The attempt threw before the backend could answer: the one case with no typed reason to word. */
function announceBreakdown(e: unknown): DirectConnectOutcome {
  log.error('Direct SMB connection attempt broke down: {error}', { error: String(e) })
  addToast(tString('fileExplorer.pane.directConnectionUnavailableToast'), { level: 'error' })
  return 'stillOnOsMount'
}
