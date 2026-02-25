/**
 * Cross-platform confirmation dialog utility.
 * Uses Tauri's native dialog API which works properly in all contexts.
 */

import { ask } from '@tauri-apps/plugin-dialog'

/**
 * Show a confirmation dialog with OK/Cancel buttons.
 * Uses Tauri's native dialog for reliable behavior.
 *
 * @param message - The message to display
 * @param title - Optional title for the dialog (defaults to 'Confirm')
 * @returns Promise that resolves to true if confirmed, false otherwise
 */
export async function confirmDialog(message: string, title = 'Confirm'): Promise<boolean> {
    // cancelLabel must be 'Cancel' so macOS assigns the ESC key equivalent to it.
    // The default 'No' label doesn't get ESC on NSAlert.
    return ask(message, { title, kind: 'warning', okLabel: 'OK', cancelLabel: 'Cancel' })
}
