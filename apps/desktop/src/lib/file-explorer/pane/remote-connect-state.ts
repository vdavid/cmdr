/**
 * What `RemoteConnectView` renders, as a closed union.
 *
 * Its own module rather than the component's, so a `*.svelte.ts` factory and a
 * plain `*.ts` test can both name the type without importing a component.
 *
 * ❗ M1 ships the two states a place can be in before the sign-in sheet exists.
 * The rest of D8's list (`waiting_for_device`, `signed_out`, `host_key_changed`,
 * `gave_up`) lands with the milestones that can act on them; adding a variant
 * here without its handler would put a button on screen that does nothing, which
 * is the one thing this view refuses to do.
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
