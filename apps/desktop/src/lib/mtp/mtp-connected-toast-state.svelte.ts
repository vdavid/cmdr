/**
 * Module-level slot for the name of the most recently connected MTP device. The
 * connection listener sets this right before `addToast(MtpConnectedToastContent, ...)`
 * so the toast can render the name without prop bridging. Read it reactively via
 * `getLastConnectedDeviceName()`.
 *
 * Lives in a `.svelte.ts` module so its types resolve across imports; a
 * `.svelte` module export is seen as `any`.
 */
let lastConnectedDeviceName = $state('MTP device')

export function setLastConnectedDeviceName(name: string): void {
  lastConnectedDeviceName = name
}

export function getLastConnectedDeviceName(): string {
  return lastConnectedDeviceName
}
