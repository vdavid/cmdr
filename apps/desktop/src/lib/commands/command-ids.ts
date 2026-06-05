/**
 * The closed tuple of every command id, and the `CommandId` union derived from it.
 *
 * This is the dependency root of the command layer: `types.ts` and
 * `command-registry.ts` both import it, neither is imported back, so the import
 * graph stays acyclic for the `import-cycles` check.
 *
 * ## Why a separate tuple instead of `commands.map(c => c.id)`
 *
 * The registry array is a mutable `Command[]` — `updateLicenseCommandName`
 * rewrites an entry's `.name` in place, and `getPaletteCommands()` plus the
 * shortcuts conflict-detector consume it as a mutable `Command[]`. Putting
 * `as const satisfies readonly Command[]` on the array to derive ids would make
 * every element property `readonly` at the type level and break those writers.
 * So the id union comes from this standalone `as const` tuple instead.
 *
 * ## Keeping the tuple and the registry in sync
 *
 * Two guards, both directions:
 * - `command-registry.ts` declares `commands: Command[]` whose `Command.id` is
 *   `CommandId`, so a registry entry whose id isn't in this tuple fails to
 *   compile (tuple ⊇ registry).
 * - `command-registry.test.ts` asserts `new Set(COMMAND_IDS)` equals
 *   `new Set(commands.map(c => c.id))`, catching a tuple id with no registry
 *   entry (registry ⊇ tuple).
 *
 * Add a command → add its id here AND add the registry entry. Both guards fire
 * if you forget one.
 */
export const COMMAND_IDS = [
  // App scope
  'app.quit',
  'app.hide',
  'app.hideOthers',
  'app.showAll',
  'app.about',
  'app.licenseKey',
  'app.commandPalette',
  'app.settings',
  'app.checkForUpdates',
  'cmdr.openOnboarding',
  'help.sendErrorReport',

  // Search
  'search.open',

  // Navigation (Go to path)
  'nav.goToPath',

  // Downloads
  'downloads.goToLatest',

  // View commands
  'view.showHidden',
  'view.briefMode',
  'view.fullMode',

  // Zoom (text size)
  'view.zoom.set75',
  'view.zoom.set100',
  'view.zoom.set125',
  'view.zoom.set150',
  'view.zoom.in',
  'view.zoom.out',

  // Sort commands
  'sort.byName',
  'sort.byExtension',
  'sort.byModified',
  'sort.bySize',
  'sort.byCreated',
  'sort.ascending',
  'sort.descending',
  'sort.toggleOrder',

  // Pane commands
  'pane.switch',
  'pane.swap',
  'pane.leftVolumeChooser',
  'pane.rightVolumeChooser',
  'pane.copyPathLeftToRight',
  'pane.copyPathRightToLeft',

  // Tab commands
  'tab.new',
  'tab.close',
  'tab.reopen',
  'tab.next',
  'tab.prev',
  'tab.togglePin',
  'tab.closeOthers',

  // File-list navigation commands
  'nav.up',
  'nav.down',
  'nav.open',
  'nav.parent',
  'nav.home',
  'nav.end',
  'nav.pageUp',
  'nav.pageDown',
  'nav.back',
  'nav.forward',

  // Brief mode specific
  'nav.left',
  'nav.right',

  // Full mode specific
  'nav.firstInFull',
  'nav.lastInFull',

  // File action commands
  'file.rename',
  'file.view',
  'file.edit',
  'file.copy',
  'file.move',

  // Edit commands (clipboard)
  'edit.copy',
  'edit.cut',
  'edit.paste',
  'edit.pasteAsMove',
  'file.newFolder',
  'file.newFile',
  'file.delete',
  'file.deletePermanently',
  'file.showInFinder',
  'file.copyPath',
  'file.copyCurrentDirectoryPath',
  'file.copyFilename',
  'file.getInfo',
  'file.quickLook',
  'file.contextMenu',
  'cloud.makeOffline',
  'cloud.removeDownload',

  // Selection commands
  'selection.toggle',
  'selection.toggleAndDown',
  'selection.selectAll',
  'selection.deselectAll',
  'selection.selectFiles',
  'selection.deselectFiles',

  // Network browser
  'network.selectHost',
  'network.refresh',

  // Share browser
  'share.back',
  'share.selectShare',

  // Volume chooser
  'volume.select',
  'volume.close',

  // About window
  'about.openWebsite',
  'about.openUpgrade',
  'about.close',

  // Command palette modal
  'palette.up',
  'palette.down',
  'palette.execute',
  'palette.close',
] as const

/** Closed union of every command id (invariant A3). */
export type CommandId = (typeof COMMAND_IDS)[number]

const COMMAND_ID_SET: ReadonlySet<string> = new Set(COMMAND_IDS)

/**
 * Runtime narrowing guard for the un-typed string edges where command ids enter
 * the frontend: the Rust `execute-command` event payload, the shortcut reverse
 * lookup, and the selection-dialog command prop. Prefer this over an
 * `as CommandId` cast — a cast would let a stale Rust id slip through to the
 * dispatcher's `default`, silently no-op, and never be caught.
 */
export function isCommandId(value: string): value is CommandId {
  return COMMAND_ID_SET.has(value)
}
