/**
 * Finder-tag color handlers (macOS): the seven keyboard-assignable "toggle <color>
 * tag" commands. Each toggles one system color on the FOCUSED pane's selection (or
 * the cursor entry when nothing is selected), via the explorer API. The context-menu
 * circles are a separate path (Rust-side, on the right-clicked set), so these only
 * cover the keyboard/palette trigger.
 *
 * Color indices match the backend and `tag-dots-utils.ts`: 1 grey … 7 orange.
 */
import type { CommandHandlerContext, CommandHandlerRecord } from './types'

function toggleTag(color: number, { explorerRef }: CommandHandlerContext): void {
  void explorerRef?.toggleTagOnFocusedSelection(color)
}

export const tagHandlers = {
  'tags.toggleGrey': (h) => { toggleTag(1, h); },
  'tags.toggleGreen': (h) => { toggleTag(2, h); },
  'tags.togglePurple': (h) => { toggleTag(3, h); },
  'tags.toggleBlue': (h) => { toggleTag(4, h); },
  'tags.toggleYellow': (h) => { toggleTag(5, h); },
  'tags.toggleRed': (h) => { toggleTag(6, h); },
  'tags.toggleOrange': (h) => { toggleTag(7, h); },
} satisfies Partial<CommandHandlerRecord>
