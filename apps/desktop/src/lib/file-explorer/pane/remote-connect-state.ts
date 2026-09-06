/**
 * What `RemoteConnectView` renders, as a closed union.
 *
 * Its own module rather than the component's, so a `*.svelte.ts` factory and a
 * plain `*.ts` test can both name the type without importing a component.
 *
 * ❗ A variant lands only once something can ACT on it. Adding one before its
 * handler puts a button on screen that does nothing, which is the one thing this
 * view refuses to do. `waiting_for_device` waits for the ADB work, and `gave_up`
 * waits for the milestone that retires `VolumeUnreachableBanner`'s `smbGaveUp`
 * variant, so the two of them don't render the same thing twice.
 */
export type RemoteConnectState =
  /** A dial or a reconnect is running. `cancel` calls it off. */
  | { kind: 'connecting'; cancel: () => void }
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
   */
  | { kind: 'signed_out'; signIn: () => void }
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
