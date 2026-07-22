/**
 * The context the dispatch core threads into every command handler: a getter for
 * the live `ExplorerAPI` and the dialog-visibility callbacks the app wires up.
 *
 * Lives in its own leaf so both the dispatch core (`command-dispatch.ts`) and the
 * family handler modules (`command-handlers/`) can import it without a cycle: the
 * core imports the handlers, the handlers import this, this imports neither.
 */
import type { ExplorerAPI } from './explorer-api'

/** Callbacks for toggling dialog visibility from command dispatch */
export interface CommandDispatchDialogs {
  showCommandPalette: (show: boolean) => void
  showSearchDialog: (show: boolean) => void
  /**
   * Opens or closes the "Go to path" dialog. The open path is guarded against
   * the menu double-dispatch (a ⌘G accelerator can fire both the menu event and
   * the JS keydown); the callback no-ops when already open.
   */
  showGoToPathDialog: (show: boolean) => void
  showAboutWindow: (show: boolean) => void
  showLicenseKeyDialog: (show: boolean) => void
  /**
   * Opens or closes the Selection dialog. `'add'` opens "Select files…",
   * `'remove'` opens "Deselect files…", `null` closes.
   */
  showSelectionDialog: (mode: 'add' | 'remove' | null) => void
  /**
   * Opens the onboarding wizard for re-entry from the `Cmdr > Onboarding…`
   * menu item or the `cmdr.openOnboarding` command palette command. No-op when
   * the wizard is already open.
   *
   * Resolves once the wizard is actually up: it loads settings and probes for
   * Full Disk Access first, so a caller that acts on the open wizard (the dialog
   * gallery's per-step preview) has to be able to wait for it.
   */
  openOnboarding: () => Promise<void>
}

export interface CommandDispatchContext {
  getExplorer: () => ExplorerAPI | undefined
  dialogs: CommandDispatchDialogs
}
