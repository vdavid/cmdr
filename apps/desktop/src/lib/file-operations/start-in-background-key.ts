/**
 * The setup dialogs' F2 (copy, move, compress, and trash): confirm, and start the
 * operation with no progress dialog. Plain F2 only: `⇧F2` and `⌘F2` are other
 * combos, and on the delete dialog Shift is the permanent-delete modifier.
 *
 * Dialog-scoped, like the progress dialog's F2 and ❌ never a command-registry
 * binding: F2 is globally `file.rename`, and `ModalDialog`'s overlay stops every
 * keydown before it reaches the global handler, so the scope is structural.
 */
export function isStartInBackgroundKey(event: KeyboardEvent): boolean {
  return event.key === 'F2' && !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey
}
