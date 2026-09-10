/**
 * The context the dispatch core threads into every command handler: a getter for
 * the live `ExplorerAPI`, the dialog-visibility callbacks the app wires up, and the
 * road the dispatch came in by.
 *
 * Lives in its own leaf so both the dispatch core (`command-dispatch.ts`) and the
 * family handler modules (`command-handlers/`) can import it without a cycle: the
 * core imports the handlers, the handlers import this, this imports neither.
 */
import type { CommandDispatchArgs, CommandId } from '$lib/commands'
import type { ExplorerAPI } from './explorer-api'

/**
 * Every road a command comes in by. Each gets its own dispatcher, bound once in `+page.svelte`
 * (`dispatchers`), so no caller can dispatch without saying which road it is.
 *
 * - `keyboard`: the document keydown resolver (`global-keydown.ts`).
 * - `menu`: the `execute-command` relay (the native menu bar, the Dock menu, and other windows),
 *   plus the menu's own `view-mode-changed` and `menu-sort` events.
 * - `mouse`: the mouse's back and forward side buttons.
 * - `palette`: a command palette row.
 * - `explorer`: the explorer's own controls (the F-key bar, a pane's selection keys).
 * - `mcp`: the MCP adapter.
 */
export const DISPATCH_SOURCES = ['keyboard', 'menu', 'mouse', 'palette', 'explorer', 'mcp'] as const
export type DispatchSource = (typeof DISPATCH_SOURCES)[number]

/** What's up in the main window that a command could land behind. */
export interface DialogsOnScreen {
  /** A soft dialog, or an explorer overlay (inline rename, the volume chooser, a confirmation). */
  dialogOpen: boolean
  /** The command palette, which is its own overlay. */
  paletteOpen: boolean
}

/** Dispatches one command down one road; arg-carrying ids take their typed payload. */
export type CommandDispatcher = <K extends CommandId>(commandId: K, ...args: CommandDispatchArgs<K>) => Promise<void>

/** One dispatcher per road. A new road is a compile error until `+page.svelte` wires it. */
export type CommandDispatchers = Readonly<Record<DispatchSource, CommandDispatcher>>

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
  /** The road this dispatch came in by, read by the cross-source dedup and the dialog gate. */
  source: DispatchSource
  /** Live read of what's on screen, for the dialog gate (`dialog-command-gate.ts`). */
  getDialogsOnScreen: () => DialogsOnScreen
}
