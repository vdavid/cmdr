/**
 * The words for every way opening a phone over ADB can stop, and what the pane
 * offers next.
 *
 * ❗ A `Record` over the backend's variant names, so a new `AdbConnectError`
 * arm can't reach a person wordless, and every key appears here as a literal
 * (which is what keeps `desktop-message-keys-unused` honest).
 *
 * Writing rules: `docs/guides/error-handling.md` § "Writing rules". ❌ Never the
 * words "error" or "failed", ❌ never a serial, and ❌ never a backend
 * diagnostic ("adb server", "sync service", "transport"): the backend's own
 * string goes to the log, and this is what a person reads.
 *
 * The refusal twin for servers is `$lib/servers/connect-refusals.ts`; the two
 * stay apart because a phone's reasons and a server's share nothing but shape.
 */

import { tString } from '$lib/intl/messages.svelte'
import type { AdbConnectOutcomeError } from '$lib/ipc/bindings'
import type { MessageKey } from '$lib/intl/keys.gen'

/** What a stopped dial leaves the pane to do. */
export type AdbConnectRecovery =
  /** A second attempt can clear it with nothing else changing. */
  | 'retry'
  /** Only Settings can fix it: there is no `adb` to talk to. */
  | 'open_settings'
  /**
   * Nothing on screen can change it. ❗ The pane offers NO button rather than a
   * "Try again" that is guaranteed to fail again.
   */
  | 'none'

/**
 * What the pane does with a dial that came back.
 *
 * Three shapes rather than one sentence, because two of the variants are not
 * refusals at all: a cancel is the user's own doing and says nothing, and a
 * phone still showing its prompt is a WAIT (the row turns ready on its own once
 * the user taps Allow, and the pane walks in from there).
 */
export type AdbConnectOutcome =
  /** ❗ Says nothing: the user pressed the button. */
  | { kind: 'silent' }
  /** `RemoteConnectView`'s `waiting_for_device`, which proceeds by itself. */
  | { kind: 'waiting'; reason: string; hint: string }
  /** `RemoteConnectView`'s `refused`, with whatever `recovery` allows. */
  | { kind: 'refused'; sentence: string; recovery: AdbConnectRecovery }

/** Every refusing variant's sentence and what it leaves to offer. */
const REFUSALS: Record<
  Exclude<AdbConnectOutcomeError['type'], 'cancelled' | 'unauthorized'>,
  { key: MessageKey; recovery: AdbConnectRecovery }
> = {
  adbNotInstalled: { key: 'adb.connect.adbNotInstalled', recovery: 'open_settings' },
  serverUnreachable: { key: 'adb.connect.serverUnreachable', recovery: 'retry' },
  // The phone left. Plugging it back in is the fix, and it produces a fresh row.
  deviceGone: { key: 'adb.connect.deviceGone', recovery: 'none' },
  // Android 6 and older speak no `shell,v2`. No button changes that.
  deviceTooOld: { key: 'adb.connect.deviceTooOld', recovery: 'none' },
  timedOut: { key: 'adb.connect.timedOut', recovery: 'retry' },
  transport: { key: 'adb.connect.transport', recovery: 'retry' },
}

/** What the pane should show for `error`. */
export function readAdbConnectOutcome(error: AdbConnectOutcomeError): AdbConnectOutcome {
  if (error.type === 'cancelled') return { kind: 'silent' }
  if (error.type === 'unauthorized') return waitingForTheAllowTap()
  const { key, recovery } = REFUSALS[error.type]
  return { kind: 'refused', sentence: tString(key), recovery }
}

/**
 * The wait for the phone's own "Allow USB debugging?" prompt, worded once.
 *
 * Two producers reach it: a dial that came back `unauthorized`, and a row that
 * was already `waiting_for_authorization` when the pane landed on it (which is
 * never dialed at all — the answer is known in advance).
 */
export function waitingForTheAllowTap(): Extract<AdbConnectOutcome, { kind: 'waiting' }> {
  return {
    kind: 'waiting',
    reason: tString('adb.connect.unauthorized'),
    hint: tString('adb.connect.waitingHint'),
  }
}
