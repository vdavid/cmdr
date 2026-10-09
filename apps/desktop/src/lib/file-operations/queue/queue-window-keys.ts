/**
 * The queue window's own keys. Esc closes it, and so does F2: the progress
 * dialog's F2 sends an operation to the queue and opens this window, so the
 * same key sends the window itself to the background (Total Commander's
 * background queue, out of sight until you ask for it again).
 */

/** What a keydown in the queue window asks for, or `null` for nothing of its own. */
export function queueWindowKeyAction(event: KeyboardEvent): 'close' | null {
  if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return null
  return event.key === 'Escape' || event.key === 'F2' ? 'close' : null
}
