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

import type { Command, CommandSource } from './types'
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
 * The fixed-key commands: their keys are hardcoded in the owning component's
 * keydown handler (FilePane arrows, palette navigation, modal Enter/Escape) and
 * never consult the shortcuts store, so a customization would be a no-op
 * illusion — the new key wouldn't fire and the built-in key wouldn't release.
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
  'network.selectHost',
  'share.back',
  'share.selectShare',
  'file.contextMenu',
] as const

/**
 * Whether the user already has a license, driving the `app.licenseKey` command's
 * name (`See license details` vs `Enter license key`). The label depends on
 * runtime license state, so it can't be a single static key. `updateLicenseCommandName`
 * flips this; the resolved command's `name` getter reads it live. Kept in sync
 * with the native menu's license item.
 */
let hasExistingLicense = true

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

/**
 * Resolves an authored `CommandSource` into a `Command` whose `name` (and, where
 * present, `description`) are getters that read the catalog through `t()` at
 * access time, so palette/menu/shortcut consumers stay unchanged and reactivity
 * holds in markup. `app.licenseKey` resolves its name from one of two keys based
 * on the live `hasExistingLicense` flag (`updateLicenseCommandName` flips it).
 */
function resolveCommand(src: CommandSource): Command {
  const { nameKey, descriptionKey, ...rest } = src
  const cmd = {
    ...rest,
    get name(): string {
      if (rest.id === 'app.licenseKey') {
        return tString(
          hasExistingLicense ? 'commands.appLicenseKey.seeDetails.label' : 'commands.appLicenseKey.enterKey.label',
        )
      }
      return tString(nameKey)
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
 * the catalog, so there's nothing to mutate in place anymore (the license name
 * is driven by `hasExistingLicense` via `updateLicenseCommandName`).
 */
export const commands: Command[] = commandSources.map(resolveCommand)

/** Get all commands that should appear in the command palette */
export function getPaletteCommands(): Command[] {
  return commands.filter((c) => c.showInPalette)
}

/**
 * Update the license command name based on whether a license exists. Keeps the
 * command palette in sync with the native menu label. The `app.licenseKey`
 * command's `name` getter reads `hasExistingLicense` live, so flipping this flag
 * re-resolves the catalog label on the next read.
 */
export function updateLicenseCommandName(hasLicense: boolean): void {
  hasExistingLicense = hasLicense
}
