/**
 * Single source of truth for all overlay (soft) dialog IDs, and for what each one
 * does to a command that would START a file operation.
 *
 * Adding a new ModalDialog with a `dialogId` not in this list produces a type error.
 * The list is registered with the Rust backend at startup so the MCP
 * "available dialogs" resource stays in sync automatically, and so the MCP file-op
 * tools can refuse honestly while a blocking dialog is up.
 *
 * The gate's scope and the reasoning behind each `whileOpen` verdict:
 * `$lib/file-explorer/pane/DETAILS.md` § "The operation-start gate".
 */

/**
 * What a dialog does to the commands that START a file operation (copy, move,
 * compress, delete, new folder, new file) while it is on screen.
 *
 * ❌ NOT to the commands that steer one already running: cancel, pause, resume,
 * rollback, and answering a name clash all keep working with the progress dialog
 * up, which is exactly when a user reaches for them.
 */
export type OperationGate = { readonly blocks: true } | { readonly blocks: false; readonly reason: string }

/** The default: the command is refused and says which dialog is in the way. */
export const BLOCKS_OPERATIONS: OperationGate = { blocks: true }

/** The opt-out, which has to carry its reason so nobody has to relitigate it. */
export function allowsOperations(reason: string): OperationGate {
  return { blocks: false, reason }
}

/**
 * One registered soft dialog. `whileOpen` is REQUIRED: a new dialog doesn't
 * compile until its author has answered whether an operation may start behind it.
 */
interface SoftDialogDeclaration {
  readonly id: string
  readonly description?: string
  readonly whileOpen: OperationGate
}

/**
 * A dialog hosted outside the main window leaves the main window's single modal
 * slot free, and its decision has nothing to do with the panes, so it lets an
 * operation start. Every main-window dialog blocks: the window shows one modal at
 * a time, and a confirmation stacked over the one the user is reading is how the
 * silent no-op this gate exists to close got in.
 */
const NOT_IN_THE_MAIN_WINDOW = 'Hosted in another window, so the main window has no modal up and no decision to lose.'

export const SOFT_DIALOG_REGISTRY = [
  { id: 'about', whileOpen: BLOCKS_OPERATIONS },
  {
    id: 'acknowledgements',
    description: 'Credits the open-source libraries Cmdr is built on',
    whileOpen: BLOCKS_OPERATIONS,
  },
  { id: 'alert', whileOpen: BLOCKS_OPERATIONS },
  {
    id: 'commercial-reminder',
    description: 'Periodic reminder for commercial licensing',
    whileOpen: BLOCKS_OPERATIONS,
  },
  {
    id: 'transfer-confirmation',
    description: 'Opened by the copy/move tool, not directly',
    whileOpen: BLOCKS_OPERATIONS,
  },
  { id: 'transfer-error', description: 'Shown after a copy/move failure', whileOpen: BLOCKS_OPERATIONS },
  { id: 'transfer-progress', description: 'Active during a copy/move operation', whileOpen: BLOCKS_OPERATIONS },
  {
    id: 'rollback-confirmation',
    description: 'Asks before Rollback deletes what a running copy or move has written',
    whileOpen: BLOCKS_OPERATIONS,
  },
  {
    id: 'operation-conflict',
    description: 'Asks how to handle a name clash in an operation running with no progress dialog in front of it',
    whileOpen: BLOCKS_OPERATIONS,
  },
  {
    id: 'archive-password',
    description: 'Prompts for an encrypted archive password before extracting',
    whileOpen: BLOCKS_OPERATIONS,
  },
  { id: 'expiration', description: 'Shown when a commercial license expires', whileOpen: BLOCKS_OPERATIONS },
  { id: 'onboarding', description: 'First-launch (and re-openable) setup wizard', whileOpen: BLOCKS_OPERATIONS },
  { id: 'license', description: 'License key entry and viewing', whileOpen: BLOCKS_OPERATIONS },
  {
    id: 'mkdir-confirmation',
    description: 'Opened by the mkdir tool, not directly',
    whileOpen: BLOCKS_OPERATIONS,
  },
  {
    id: 'new-file-confirmation',
    description: 'Opened by the new-file tool, not directly',
    whileOpen: BLOCKS_OPERATIONS,
  },
  { id: 'mtp-permission', description: 'Linux MTP USB permission troubleshooting', whileOpen: BLOCKS_OPERATIONS },
  { id: 'ptpcamerad', description: 'MTP device connection troubleshooting', whileOpen: BLOCKS_OPERATIONS },
  {
    id: 'rename-conflict',
    description: 'Shown when renaming would overwrite an existing file',
    whileOpen: BLOCKS_OPERATIONS,
  },
  {
    id: 'extension-change',
    description: 'Shown when a rename changes the file extension',
    whileOpen: BLOCKS_OPERATIONS,
  },
  {
    id: 'crash-report',
    description: 'Post-crash dialog offering to send a crash report',
    whileOpen: BLOCKS_OPERATIONS,
  },
  {
    id: 'error-report',
    description: 'Preview-and-send dialog for user-initiated error reports',
    whileOpen: BLOCKS_OPERATIONS,
  },
  { id: 'feedback', description: 'Open-beta "Send feedback" dialog', whileOpen: BLOCKS_OPERATIONS },
  { id: 'whats-new', description: 'Post-update changelog summary popup', whileOpen: BLOCKS_OPERATIONS },
  {
    id: 'operation-log',
    description: 'Alpha history of file operations, with expandable per-operation items',
    whileOpen: BLOCKS_OPERATIONS,
  },
  {
    id: 'delete-confirmation',
    description: 'Opened by the delete tool, not directly',
    whileOpen: BLOCKS_OPERATIONS,
  },
  {
    id: 'delete-ai-model',
    description: 'Confirmation before deleting the local AI model',
    whileOpen: allowsOperations(NOT_IN_THE_MAIN_WINDOW),
  },
  { id: 'search', description: 'Whole-drive file search', whileOpen: BLOCKS_OPERATIONS },
  {
    id: 'go-to-path',
    description: 'Jump the focused pane to a typed or recent path',
    whileOpen: BLOCKS_OPERATIONS,
  },
  {
    id: 'selection-add',
    description: '"Select files…" (+): adds matching files to the pane selection',
    whileOpen: BLOCKS_OPERATIONS,
  },
  {
    id: 'selection-remove',
    description: '"Deselect files…" (-): removes matching files from the pane selection',
    whileOpen: BLOCKS_OPERATIONS,
  },
  {
    id: 'bulk-rename-review',
    description: 'Reviews an Ask Cmdr rename proposal before any files change',
    whileOpen: BLOCKS_OPERATIONS,
  },
  { id: 'connect-to-server', description: 'Manual SMB server address entry', whileOpen: BLOCKS_OPERATIONS },
  {
    id: 'viewer-copy-confirm',
    description: 'Confirms copying a 10 to 100 MB selection from the file viewer',
    whileOpen: allowsOperations(NOT_IN_THE_MAIN_WINDOW),
  },
  {
    id: 'viewer-copy-refuse',
    description: 'Tells the user a > 100 MB viewer selection is too large to copy',
    whileOpen: allowsOperations(NOT_IN_THE_MAIN_WINDOW),
  },
  {
    id: 'drive-index-stale',
    description: 'One-time explainer the first time an external drive index goes stale',
    whileOpen: BLOCKS_OPERATIONS,
  },
  {
    id: 'quit-confirmation',
    description: 'Asks whether to quit while copies or other operations are still running, on a countdown',
    whileOpen: BLOCKS_OPERATIONS,
  },
] as const satisfies readonly SoftDialogDeclaration[]

export type SoftDialogId = (typeof SOFT_DIALOG_REGISTRY)[number]['id']

/** Whether starting a file operation is refused while `id` is open. */
export function dialogBlocksOperations(id: SoftDialogId): boolean {
  return SOFT_DIALOG_REGISTRY.find((d) => d.id === id)?.whileOpen.blocks ?? true
}
