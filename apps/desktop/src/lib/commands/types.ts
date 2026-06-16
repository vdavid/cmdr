/**
 * Command types for the command palette and shortcut configuration.
 */

import type { ViewMode } from '$lib/app-status-store'
import type { BadgeStatus } from '$lib/feature-status'
import type { MessageKey } from '$lib/intl/keys.gen'
import type { CommandId } from './command-ids'

export type { CommandId } from './command-ids'

/** Command scope hierarchy - determines where command is available */
export type CommandScope =
  | 'App' // Global commands (⌘Q works everywhere)
  | 'Main window' // Main window commands
  | 'Main window/File list' // File list navigation/actions
  | 'Main window/Brief mode' // Brief mode specific
  | 'Main window/Full mode' // Full mode specific
  | 'Main window/Network' // Network browser
  | 'Main window/Share browser' // Share browser
  | 'Main window/Volume chooser' // Volume dropdown
  | 'About window' // About window commands
  | 'Onboarding' // FDA prompt
  | 'Command palette' // Command palette modal

/**
 * Marker for a command that takes no dispatch argument. A distinct named type
 * (not `void`, which `@typescript-eslint/no-invalid-void-type` bans outside
 * return positions) so `CommandArgs` can map arg-less ids to it and
 * `CommandDispatchArgs` can branch on it.
 */
export type NoCommandArgs = undefined

/** Which pane a per-pane command targets. */
export type PaneId = 'left' | 'right'

/** Sortable column (mirrors `ExplorerAPI.setSort`). */
export type SortColumn = 'name' | 'extension' | 'size' | 'modified' | 'created'

/** Selection mode for the MCP `select` tool. */
export type McpSelectMode = 'replace' | 'add' | 'subtract'

/** Tab action for the MCP `tab` tool. */
export type McpTabAction = 'new' | 'close' | 'close_others' | 'activate' | 'reopen' | 'set_pinned'

/** Dialog kind the MCP `dialog confirm` tool can confirm. */
export type ConfirmDialogType = 'transfer-confirmation' | 'delete-confirmation'

/**
 * Override slot for commands whose dispatch payload is REQUIRED. Each key is the
 * command's id; its value is the payload shape. PR1 forbids dead arg types, so
 * only commands something actually dispatches with args appear.
 *
 * `view.setMode` sets a SPECIFIC pane's view mode (unlike the focused-pane
 * `view.briefMode` / `view.fullMode`). `fromMenu` discriminates the two callers
 * that share it: the native-menu `view-mode-changed` event (`fromMenu: true` →
 * skip `pushViewMenuState`, the menu already toggled its CheckMenuItem) and the
 * MCP `set_view_mode` tool (`fromMenu: false` → push the menu state, since
 * nothing toggled it).
 *
 * The rest carry the per-pane MCP payloads the focused-pane registry commands
 * can't express.
 */
export interface CommandArgsOverrides {
  'view.setMode': { pane: PaneId; mode: ViewMode; fromMenu: boolean }
  'sort.set': { pane: PaneId; column: SortColumn; order: 'asc' | 'desc' }
  'selection.mcpSelect': { pane: PaneId; start: number; count: number | 'all'; mode: McpSelectMode }
  'selection.mcpSelectByNames': { pane: PaneId; names: string[]; mode: McpSelectMode }
  'cursor.moveTo': { pane: PaneId; to: number | string }
  'cursor.scrollTo': { pane: PaneId; index: number }
  'volume.selectByName': { pane: PaneId; name: string }
  'tab.mcpAction': { pane: PaneId; action: McpTabAction; tabId?: string; pinned?: boolean }
  'dialog.confirm': { type: ConfirmDialogType; onConflict?: string }
}

/**
 * Override slot for commands whose dispatch payload is OPTIONAL. These are
 * dispatched arg-less from the F-key bar / palette / keyboard (open the dialog
 * with no preset) AND with a payload from the MCP `copy`/`move`/`delete` tools
 * (the AI can pre-answer the conflict policy / auto-confirm). `CommandDispatchArgs`
 * resolves these to a `[args?]` tuple so both call shapes type-check.
 */
export interface CommandArgsOptionalOverrides {
  'file.copy': { autoConfirm?: boolean; onConflict?: string }
  'file.move': { autoConfirm?: boolean; onConflict?: string }
  'file.delete': { autoConfirm?: boolean }
}

/**
 * Per-command argument map. Commands in `CommandArgsOverrides` /
 * `CommandArgsOptionalOverrides` resolve to their typed payload; every other id
 * resolves to `NoCommandArgs`, so `dispatch('file.rename')` needs no second
 * argument. A conditional mapped type (not an intersection) keeps the arg-less
 * default `undefined` rather than collapsing overridden keys to `never`.
 */
export type CommandArgs = {
  [K in CommandId]: K extends keyof CommandArgsOverrides
    ? CommandArgsOverrides[K]
    : K extends keyof CommandArgsOptionalOverrides
      ? CommandArgsOptionalOverrides[K]
      : NoCommandArgs
}

/**
 * Tuple form of a command's dispatch arguments: `[]` for arg-less commands,
 * `[args]` for required-payload ones, `[args?]` for optional-payload ones. Lets a
 * single `dispatch<K>(id, ...args)` signature stay terse for the common case
 * while type-checking the rest.
 *
 * Distributes over `K` (naked type param in the conditional) so a call site
 * holding a broad `CommandId` resolves to `[] | [args] | [args?]` — i.e. an
 * arg-less dispatch (`handleCommandExecute(commandId)`) stays valid while an
 * arg-carrying literal id (`handleCommandExecute('view.setMode', payload)`) still
 * requires its payload. Without the distribution, `CommandArgs[CommandId]` is a
 * union that doesn't extend `NoCommandArgs`, which would wrongly force a payload
 * on every broad-id call site.
 */
export type CommandDispatchArgs<K extends CommandId> = K extends CommandId
  ? K extends keyof CommandArgsOptionalOverrides
    ? [args?: CommandArgs[K]]
    : CommandArgs[K] extends NoCommandArgs
      ? []
      : [args: CommandArgs[K]]
  : never

/** A command definition */
export interface Command {
  /** Unique identifier (like 'file.open', 'nav.parent') */
  id: CommandId
  /** Display name shown in palette */
  name: string
  /** Hierarchical scope */
  scope: CommandScope
  /** Show in command palette? (false for low-level nav like ↑/↓) */
  showInPalette: boolean
  /** Keyboard shortcuts (like ['⌘⇧P', 'F1']) */
  shortcuts: string[]
  /**
   * Stability badge shown in the palette row (uppercase ALPHA / BETA pill).
   * Derive it from `getBadgeStatus(id)` in `$lib/feature-status` so the
   * repo-root `feature-status.json` stays the single source of truth.
   * Omitted (stable features) renders no badge.
   */
  status?: BadgeStatus
  /**
   * macOS owns this command's behavior AND its accelerator via a
   * `PredefinedMenuItem` (`terminate:`, `hide:`, `hideOtherApplications:`,
   * `unhideAllApplications:`). Cmdr can neither rebind nor intercept it, so the
   * shortcuts editor renders it read-only and the store refuses to customize it.
   * Set only on the four `NATIVE_SHORTCUT_COMMAND_IDS` (Family 1 dispatch-exempt).
   */
  nativeShortcut?: true
  /**
   * The command's key is hardcoded in its owning component's keydown handler
   * (FilePane arrows, palette navigation, modal Enter/Escape) and never consults
   * the shortcuts store, so a customization would be a no-op illusion. The
   * shortcuts editor renders it read-only ("Fixed" badge) and the store refuses
   * to customize it. Set only on the `FIXED_KEY_COMMAND_IDS` (the Family 2/3
   * dispatch-exempt ids).
   */
  fixedKey?: true
  /** Optional description for long-form help */
  description?: string
  /**
   * Extra search terms folded into the fuzzy-search haystack so the palette finds the command
   * by synonyms not present in its name (like ['jump', 'navigate']). They never produce visible
   * highlights: matches landing in keyword text are dropped (see `fuzzy-search.ts`).
   */
  keywords?: string[]
}

/**
 * Authored form of a command, as written in `command-registry.ts`. Holds i18n
 * message KEYS (`nameKey` / `descriptionKey`) instead of English; `resolveCommand`
 * turns each source into a `Command` whose `name` / `description` are getters that
 * resolve the catalog string through `t()` at READ time. So every `command.name`
 * consumer (palette, fuzzy haystack, shortcuts list, menus) gets a rendered
 * string and reactivity works in markup, while the copy lives in
 * `messages/en/commands.json`. The id and all behavioral flags carry through
 * unchanged.
 */
export type CommandSource = Omit<Command, 'name' | 'description'> & {
  /** Message key for the command's display name (`commands.<idish>.label`). */
  nameKey: MessageKey
  /** Optional message key for the longer palette help text. */
  descriptionKey?: MessageKey
}

/** Result of a fuzzy search match */
export interface CommandMatch {
  /** The matched command */
  command: Command
  /** Indices of matched characters in command.name for highlighting */
  matchedIndices: number[]
}
