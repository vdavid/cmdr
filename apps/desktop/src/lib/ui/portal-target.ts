/**
 * Where a floating menu (`Select`, `Combobox`) mounts while it's open.
 *
 * Every menu portals: rendered inline it would inherit its ancestors' `overflow`
 * clip, `mask-image`, and stacking context, and no `z-index` escapes those. The
 * destination is `document.body`, unless a modal layer up the tree provides its
 * own root. Inside a modal, body is wrong twice over: the menu's `--z-dropdown`
 * rung sits under the modal's `--z-modal` scrim, and the layer's focus trap pulls
 * focus straight back out of a menu that lives outside it. So a modal layer calls
 * `providePortalTarget` with its overlay element (the one that carries the rung and
 * the trap, never a clipping panel inside it), and every menu beneath it lands
 * there. Callers of `Select` / `Combobox` never choose.
 */

import { getContext, setContext } from 'svelte'

const PORTAL_TARGET = Symbol('portal-target')

/**
 * Makes the calling component's subtree portal its menus into `getTarget()`. Call
 * during component initialisation. A getter, because the provider's element is
 * bound after its children initialise; menus read it only when they mount, by
 * which point it's set.
 */
export function providePortalTarget(getTarget: () => HTMLElement | undefined): void {
  setContext(PORTAL_TARGET, getTarget)
}

/**
 * The resolver a floating menu passes to Ark's `Portal` as its container: the
 * nearest provided target, else `document.body`. Call during component
 * initialisation; call the returned function when the portal mounts.
 */
export function usePortalTarget(): () => HTMLElement {
  const provided = getContext<(() => HTMLElement | undefined) | undefined>(PORTAL_TARGET)
  return () => provided?.() ?? document.body
}
