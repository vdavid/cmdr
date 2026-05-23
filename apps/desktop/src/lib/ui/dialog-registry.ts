/**
 * Single source of truth for all overlay (soft) dialog IDs.
 *
 * Adding a new ModalDialog with a `dialogId` not in this list produces a type error.
 * The list is registered with the Rust backend at startup so the MCP
 * "available dialogs" resource stays in sync automatically.
 */
export const SOFT_DIALOG_REGISTRY = [
  { id: 'about' },
  { id: 'alert' },
  { id: 'commercial-reminder', description: 'Periodic reminder for commercial licensing' },
  { id: 'transfer-confirmation', description: 'Opened by the copy/move tool, not directly' },
  { id: 'transfer-error', description: 'Shown after a copy/move failure' },
  { id: 'transfer-progress', description: 'Active during a copy/move operation' },
  { id: 'expiration', description: 'Shown when a commercial license expires' },
  { id: 'full-disk-access', description: 'Onboarding prompt for full disk access permission' },
  { id: 'onboarding', description: 'First-launch (and re-openable) setup wizard' },
  { id: 'license', description: 'License key entry and viewing' },
  { id: 'mkdir-confirmation', description: 'Opened by the mkdir tool, not directly' },
  { id: 'new-file-confirmation', description: 'Opened by the new-file tool, not directly' },
  { id: 'mtp-permission', description: 'Linux MTP USB permission troubleshooting' },
  { id: 'ptpcamerad', description: 'MTP device connection troubleshooting' },
  { id: 'rename-conflict', description: 'Shown when renaming would overwrite an existing file' },
  { id: 'extension-change', description: 'Shown when a rename changes the file extension' },
  { id: 'crash-report', description: 'Post-crash dialog offering to send a crash report' },
  { id: 'error-report', description: 'Preview-and-send dialog for user-initiated error reports' },
  { id: 'delete-confirmation', description: 'Opened by the delete tool, not directly' },
  { id: 'delete-ai-model', description: 'Confirmation before deleting the local AI model' },
  { id: 'search', description: 'Whole-drive file search' },
  { id: 'connect-to-server', description: 'Manual SMB server address entry' },
  { id: 'viewer-copy-confirm', description: 'Confirms copying a 10 to 100 MB selection from the file viewer' },
  { id: 'viewer-copy-refuse', description: 'Tells the user a > 100 MB viewer selection is too large to copy' },
] as const satisfies readonly { id: string; description?: string }[]

export type SoftDialogId = (typeof SOFT_DIALOG_REGISTRY)[number]['id']
