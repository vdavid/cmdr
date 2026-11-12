/**
 * Pure builder for the "Go to latest download" toggle description in
 * `Behavior > Notifications`. The description references the LIVE global binding
 * so the moment the user rebinds the hotkey in `Keyboard shortcuts`, the
 * toggle's helper text updates to match.
 *
 * The binding is shown the way `GlobalShortcutRow` shows it (`toDisplayShortcut`),
 * so the two never disagree (`Win` for the Windows key on Windows).
 */
import { tString } from '#lib/intl/messages.svelte.js'
import { toDisplayShortcut } from '#lib/shortcuts/key-capture.js'

export function globalGoToLatestDescription(binding: string): string {
  if (!binding) {
    return tString('downloads.toggleDescription.unbound')
  }
  return tString('downloads.toggleDescription.bound', { binding: toDisplayShortcut(binding) })
}
