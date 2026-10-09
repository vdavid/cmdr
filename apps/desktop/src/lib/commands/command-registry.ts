/**
 * Complete registry of all commands in the application.
 *
 * This is the single source of truth for:
 * - Command palette entries
 * - Keyboard shortcut documentation
 * - Future MCP server commands
 * - Settings pane shortcut configuration
 *
 * Each entry is authored as a `CommandSource` holding i18n message KEYS
 * (`nameKey` / `descriptionKey`), not English. `resolveCommand` turns each source
 * into a `Command` whose `name` / `description` resolve the catalog string through
 * `t()` at read time, so the whole `command.name` consumer surface (palette,
 * fuzzy haystack, shortcuts list, menus) is unchanged while the copy lives in
 * `messages/en/commands.json`. The command IDS stay untouched.
 */

import type { Command, CommandSource, PaletteCondition } from './types'
import type { CommandId } from './command-ids'
import { BLOCKED_BY_DIALOGS, type WhileDialogOpen } from './while-dialog-open'
import { tString } from '$lib/intl/messages.svelte'
import { appCommands } from './sources/app'
import { mainWindowCommands } from './sources/main-window'
import { fileListCommands } from './sources/file-list'
import { browsersCommands } from './sources/browsers'
import { mcpCommands } from './sources/mcp'
import { aboutWindowCommands } from './sources/about-window'
import { commandPaletteCommands } from './sources/command-palette'

/**
 * The macOS-native commands: AppKit `PredefinedMenuItem`s own BOTH the behavior
 * and the accelerator (`terminate:`, `hide:`, `hideOtherApplications:`,
 * `unhideAllApplications:`). Cmdr can neither rebind nor intercept them, so the
 * shortcuts editor renders them read-only and the store refuses to customize
 * them. Single source of truth: the registry entries below carry
 * `nativeShortcut: true` for exactly these ids (pinned by `command-registry.test.ts`),
 * and `command-handlers/types.ts` sources its Family-1 dispatch-exempt list from here.
 */
export const NATIVE_SHORTCUT_COMMAND_IDS = ['app.quit', 'app.hide', 'app.hideOthers', 'app.showAll'] as const

/**
 * The fixed-key commands: their keys are owned by the component that handles them
 * (FilePane arrows, palette navigation, modal Enter/Escape, the error screen's ⌘D)
 * rather than by the shortcuts store, so a customization would be a no-op illusion
 * — the new key wouldn't fire and the built-in key wouldn't release.
 * The shortcuts editor renders them read-only ("Fixed" badge) and the store
 * refuses to customize them. Single source of truth: the registry entries carry
 * `fixedKey: true` for exactly these ids (pinned by `command-registry.test.ts`),
 * and `command-handlers/types.ts` sources its Family-2/3 dispatch-exempt lists
 * from here.
 */
export const FIXED_KEY_COMMAND_IDS = [
  // Family 2 — per-keystroke file-list navigation (FilePane keydown).
  'nav.up',
  'nav.down',
  'nav.left',
  'nav.right',
  'nav.firstInFull',
  'nav.lastInFull',
  // Family 3 — component-scoped modal / sub-view keys.
  'palette.up',
  'palette.down',
  'palette.execute',
  'palette.close',
  'volume.select',
  'volume.close',
  'favorites.openByNumber',
  'favorites.addFromMenu',
  'multiRename.openPresets',
  'multiRename.savePreset',
  'network.selectHost',
  'share.back',
  'share.selectShare',
  // Family 4 — deliberate override. ErrorPane claims ⌘D through a CAPTURE-phase
  // document listener that runs ahead of the dispatch spine, so it wins over
  // whatever the user bound ⌘D to. Fixed because releasing the key would falsify
  // the "Technical details ⌘D" hint the error screen advertises.
  'errorPane.toggleTechnicalDetails',
] as const

// The registry data lives in `sources/`, one file per top-level scope, each
// exporting a `CommandSource[]`. They're concatenated here in the original
// authoring order (order matters: it drives palette listing and shortcut
// conflict resolution). `CommandSource.id` is the `CommandId` union derived from
// `COMMAND_IDS` in `command-ids.ts`, so an entry whose id isn't in that tuple is
// a compile error; a tuple id with no entry is caught by the set-equality test
// in `command-registry.test.ts`.
const commandSources: CommandSource[] = [
  ...appCommands,
  ...mainWindowCommands,
  ...fileListCommands,
  ...browsersCommands,
  ...mcpCommands,
  ...aboutWindowCommands,
  ...commandPaletteCommands,
]

const whileDialogOpenById = new Map<CommandId, WhileDialogOpen>(
  commandSources.map((source) => [source.id, source.whileDialogOpen]),
)

/**
 * What `id` does while a dialog or overlay is up. Read by the dispatch core's dialog gate
 * (`routes/(main)/dialog-command-gate.ts`) and the native menu's greying.
 */
export function whileDialogOpenFor(id: CommandId): WhileDialogOpen {
  // Every id has an entry (`command-registry.test.ts` pins the id sets equal), so the
  // fallback is unreachable. Refusing is the answer that can't do damage if it ever isn't.
  return whileDialogOpenById.get(id) ?? BLOCKED_BY_DIALOGS
}

/**
 * Resolves an authored `CommandSource` into a `Command` whose `name`,
 * `displayName` (and, where present, `description`) are getters reading through
 * `t()` at access time, so palette/menu/shortcut consumers stay unchanged and
 * reactivity holds in markup.
 *
 * Two of those getters carry live state, both generically — ❌ no per-id branch
 * here. A `nameKey` thunk picks between several catalog keys (`app.licenseKey`
 * flips on license state); a `displayName` resolver composes the palette's row
 * label outright, and falls back to `name` when a source declares none.
 */
function resolveCommand(src: CommandSource): Command {
  const { nameKey, descriptionKey, displayName, ...rest } = src
  const resolveName = (): string => tString(typeof nameKey === 'function' ? nameKey() : nameKey)
  const cmd = {
    ...rest,
    get name(): string {
      return resolveName()
    },
    get displayName(): string {
      return displayName ? displayName() : resolveName()
    },
  } as Command
  if (descriptionKey !== undefined) {
    Object.defineProperty(cmd, 'description', { enumerable: true, get: () => tString(descriptionKey) })
  }
  return cmd
}

/**
 * Every command, with copy resolved through the catalog. A getter-backed
 * `Command[]` (not `as const`), so `getPaletteCommands()` and the shortcuts
 * conflict detector keep a mutable `Command[]`; the names themselves come from
 * the catalog, so there's nothing to mutate in place.
 */
export const commands: Command[] = commandSources.map(resolveCommand)

/**
 * The commands the palette lists. `holds` answers each `paletteCondition`;
 * without it, a conditional command counts as shown, which is what a caller
 * asking about the palette's whole vocabulary (pruning stale recents) wants.
 */
export function getPaletteCommands(holds?: (condition: PaletteCondition) => boolean): Command[] {
  return commands.filter(
    (c) => c.showInPalette && (c.paletteCondition === undefined || holds === undefined || holds(c.paletteCondition)),
  )
}

// Re-exported from `sources/app.ts`, where the flag it flips lives beside the
// `app.licenseKey` entry that reads it. Callers (`routes/(main)/+page.svelte`)
// keep importing it from the registry.
export { updateLicenseCommandName } from './sources/app'
