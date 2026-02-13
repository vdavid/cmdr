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
    { id: 'license', description: 'License key entry and viewing' },
    { id: 'mkdir-confirmation', description: 'Opened by the mkdir tool, not directly' },
    { id: 'ptpcamerad', description: 'MTP device connection troubleshooting' },
    { id: 'rename-conflict', description: 'Shown when renaming would overwrite an existing file' },
    { id: 'extension-change', description: 'Shown when a rename changes the file extension' },
] as const satisfies readonly { id: string; description?: string }[]

export type SoftDialogId = (typeof SOFT_DIALOG_REGISTRY)[number]['id']
