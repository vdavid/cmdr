/**
 * What the document-level `contextmenu` handler should DO with a right-click.
 * The decision lives here as a pure function so it's unit-testable; `+page.svelte`
 * only performs the side effects.
 *
 * Cmdr owns right-click almost everywhere: the file rows, tabs, breadcrumb,
 * volume rows, query results, and network rows each open a native macOS menu of
 * their own, so the main window suppresses WebKit's menu by default (two menus
 * for one click, otherwise).
 *
 * Text fields are the exception: WebKit's own editing menu (Cut / Copy / Paste /
 * Select All, plus the system Services and spelling entries) is exactly the menu
 * a text field should have, and it acts on the field directly, so it can't
 * double up with the `edit.*` command handlers the way ⌘V once did.
 */
import { isTextInputTarget } from '$lib/utils/text-input-focus'

/** What `+page.svelte` should do with the right-click. */
export type GlobalContextMenuAction =
  /**
   * Let WebKit open its text-editing menu, and stop the event before any ancestor
   * handler opens a Cmdr menu instead (the inline rename editor and the volume
   * switcher's favorite-rename field sit inside rows that have their own menu).
   */
  | 'native-text-menu'
  /** `preventDefault`, so the only right-click menus outside text fields are Cmdr's. */
  | 'suppress'

/** Decides who owns this right-click: the text field under the pointer, or Cmdr. */
export function resolveGlobalContextMenuAction(event: MouseEvent): GlobalContextMenuAction {
  return isTextInputTarget(event.target) ? 'native-text-menu' : 'suppress'
}
