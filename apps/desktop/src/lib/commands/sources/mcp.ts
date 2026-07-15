/**
 * MCP-only per-pane command sources. Pure data (i18n message keys, not English); see
 * `../command-registry.ts` for how the scope arrays are concatenated into the
 * registry and resolved into `Command`s.
 */
import type { CommandSource } from '../types'

export const mcpCommands: CommandSource[] = [
  // ============================================================================
  // MCP-only per-pane commands
  // ============================================================================
  // Carry per-pane / per-option payloads the focused-pane registry commands can't
  // express. Dispatched by the MCP server's tools through the command bus; all are
  // `showInPalette: false` (no user-facing palette entry) with no shortcut.
  {
    id: 'volume.selectByName',
    nameKey: 'commands.volumeSelectByName.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'selection.mcpSelect',
    nameKey: 'commands.selectionMcpSelect.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'selection.mcpSelectByNames',
    nameKey: 'commands.selectionMcpSelectByNames.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'cursor.moveTo',
    nameKey: 'commands.cursorMoveTo.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'cursor.scrollTo',
    nameKey: 'commands.cursorScrollTo.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'pane.refresh',
    nameKey: 'commands.paneRefresh.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'nav.openUnderCursor',
    nameKey: 'commands.navOpenUnderCursor.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'tab.mcpAction',
    nameKey: 'commands.tabMcpAction.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'dialog.confirm',
    nameKey: 'commands.dialogConfirm.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
]
