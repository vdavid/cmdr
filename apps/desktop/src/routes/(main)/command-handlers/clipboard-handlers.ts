/**
 * Clipboard handlers: copy / cut / paste / paste-as-move. Each branches on
 * input-vs-file focus (`isTextInputFocused`: a focused `<input>` / `<textarea>` /
 * contenteditable routes to the native text op; otherwise the file-scope
 * clipboard runs). This is a SEPARATE focus layer from the pre-dispatch
 * text-region intercept in the core (`handleTextRegionShortcut`).
 */
import { readClipboardText } from '$lib/tauri-commands'
import { isTextInputFocused } from '$lib/utils/text-input-focus'
import type { CommandHandlerRecord } from './types'

export const clipboardHandlers = {
  'edit.copy': ({ explorerRef }) => {
    if (isTextInputFocused()) {
      // eslint-disable-next-line @typescript-eslint/no-deprecated -- No modern alternative for triggering native copy in text inputs
      document.execCommand('copy')
      return
    }
    // If the user has selected text anywhere with `user-select: text`
    // (for example, the ErrorPane), prefer copying that text over the file
    // selection. Note: the +page.svelte global keydown bail doesn't help on
    // macOS, where the native Edit > Copy menu accelerator fires before JS
    // sees the keydown; this branch is the actual entry point in that case.
    const selection = window.getSelection()
    if (selection && !selection.isCollapsed && selection.toString().length > 0) {
      void navigator.clipboard.writeText(selection.toString())
      return
    }
    void explorerRef?.copyToClipboard()
  },

  'edit.cut': ({ explorerRef }) => {
    if (isTextInputFocused()) {
      // eslint-disable-next-line @typescript-eslint/no-deprecated -- No modern alternative for triggering native cut in text inputs
      document.execCommand('cut')
      return
    }
    void explorerRef?.cutToClipboard()
  },

  'edit.paste': async ({ explorerRef }) => {
    if (isTextInputFocused()) {
      // Read clipboard text via Rust (bypasses WebKit's navigator.clipboard
      // permission popup that shows a "Paste" button the user must click).
      const text = await readClipboardText()
      if (text) {
        // eslint-disable-next-line @typescript-eslint/no-deprecated -- insertText is the only way to insert at cursor position in inputs
        document.execCommand('insertText', false, text)
      }
      return
    }
    void explorerRef?.pasteFromClipboard(false)
  },

  'edit.pasteAsMove': ({ explorerRef }) => {
    // Option+Cmd+V is not a text shortcut, so no activeElement check needed
    void explorerRef?.pasteFromClipboard(true)
  },
} satisfies Partial<CommandHandlerRecord>
