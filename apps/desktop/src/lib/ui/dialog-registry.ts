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
    { id: 'copy-confirmation', description: 'Opened by the copy tool, not directly' },
    { id: 'copy-error', description: 'Shown after a copy failure' },
    { id: 'copy-progress', description: 'Active during a copy operation' },
    { id: 'expiration', description: 'Shown when a commercial license expires' },
    { id: 'license', description: 'License key entry and viewing' },
    { id: 'mkdir-confirmation', description: 'Opened by the mkdir tool, not directly' },
    { id: 'ptpcamerad', description: 'MTP device connection troubleshooting' },
] as const satisfies readonly { id: string; description?: string }[]

export type SoftDialogId = (typeof SOFT_DIALOG_REGISTRY)[number]['id']
