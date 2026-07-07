import { tString } from '$lib/intl/messages.svelte'
import type { PastedClipboardFile } from '$lib/tauri-commands'

/**
 * Composes the paste-as-file info toast message (e.g. "Pasted clipboard text as
 * pasted.txt"). Pure, so the golden output is unit-testable.
 *
 * In its own module (not `paste-clipboard-as-file.ts`) so `PasteClipboardToastContent.svelte`
 * can import it without an import cycle: the orchestrator imports the toast
 * component, so the component must NOT reach back into the orchestrator.
 */
export function pastedAsFileMessage(kind: PastedClipboardFile['kind'], filename: string): string {
  return tString('fileExplorer.clipboard.pastedAsFile', { kind, filename })
}
