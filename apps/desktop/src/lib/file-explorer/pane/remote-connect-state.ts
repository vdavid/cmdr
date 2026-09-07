/**
 * What `RemoteConnectView` renders, as a closed union.
 *
 * Its own module rather than the component's, so a `*.svelte.ts` factory and a
 * plain `*.ts` test can both name the type without importing a component.
 *
 * ❗ A variant lands only once something can ACT on it. Adding one before its
 * handler puts a button on screen that does nothing, which is the one thing this
 * view refuses to do. `waiting_for_device` waits for the ADB work.
 *
 * ❗ There is no `gave_up` variant on purpose. A cycle that ran out of attempts
 * renders `VolumeUnreachableBanner`'s `gaveUp` variant, which is the app's one
 * "couldn't reach this" surface and already words the path, the retry, and the
 * disconnect. Two renderers for one state is worse than one in the file next
 * door.
 */
export type RemoteConnectState =
  /**
   * A dial or a reconnect is running. `cancel` calls it off.
   *
   * `cycle` is present when a BACKOFF LOOP is what's running rather than a
   * single dial: it carries the loop's own sentences, its countdown to the next
   * attempt, and the two things only a loop offers (skip the wait, or stop and
   * drop the session). ❗ Honest progress is the point — without the countdown a
   * person can't tell a slow handshake from a wedged one, and without "Try now"
   * they wait out a delay for a server they can see is back.
   */
  | { kind: 'connecting'; cancel: () => void; cycle?: RetryCycle }
  /**
   * It stopped, with a reason worth reading. `refusal` is the finished sentence
   * (`servers/connect-refusals.ts`), ❌ never a key or a backend string.
   * `disconnect` is offered only where there is a session to drop.
   */
  | { kind: 'refused'; refusal: string; retry: () => void; disconnect?: () => void }
  /**
   * The session ended because a credential is what's missing. `signIn` opens the
   * sheet, which asks what the BACKEND said to ask.
   *
   * ❗ The sheet opens on the user pressing this, ❌ never on its own when a
   * session drops: a modal stealing focus during a lid-open wake is the wrong
   * thing, and the reconnect's "silent" promise stays.
   *
   * ❗ `null` where the backend's shape is `nothing`: there is no secret a person
   * could type that would help, so the banner says what happened and offers no
   * button rather than one that can't work.
   */
  | { kind: 'signed_out'; signIn: (() => void) | null }
  /**
   * SFTP only: the server presents a different host key than the one this Mac
   * trusts, so the backend stopped.
   *
   * ❗ It offers Disconnect, ❌ not "Trust it": the fingerprint has to be SHOWN
   * before anyone can answer for it, and nothing here has it — the backend keeps
   * no pending prompt for a registered volume. Disconnecting drops the dead
   * session, and reopening the place dials afresh, which is what produces the
   * prompt the sheet's key step renders.
   */
  | { kind: 'host_key_changed'; disconnect: () => void }

/**
 * A backoff loop's own face, inside the `connecting` state.
 *
 * Plain data and callbacks: the view holds no reference to the reconnect manager,
 * so a second backend's loop renders through the same component by handing over
 * the same five things.
 */
export interface RetryCycle {
  /** What it is doing and for how long. One line each, in order, under the spinner. */
  lines: string[]
  /**
   * The wait before the next attempt, for the draining bar. ❗ `null` while an
   * attempt is actually in flight: there is no countdown to draw then, and
   * nothing to skip.
   */
  waiting: { startedAt: number; durationMs: number } | null
  /** Skips the wait and tries now. Offered only while `waiting`. */
  retryNow: () => void
  /** Stops the loop and drops the session. */
  disconnect: () => void
}
