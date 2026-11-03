/**
 * What ⌃S (`pane.quickFilter`) does to a pane: it opens the badge on the current pattern, and each
 * press after that switches the same pattern between Filter (hide what doesn't match) and Jump
 * (show everything, cursor on the match). One instance per pane, inside `FilePane`, over its two
 * typing controllers.
 *
 * A session sits above the `fileExplorer.typeToJump.mode` setting: while one is open, typing goes
 * to its kind whatever the setting says. It lasts until Esc, the × on the badge, or the pane
 * leaving the folder (`end()`), and then plain typing follows the setting again.
 *
 * ## The pattern lives with whichever controller is on
 *
 * On Filter it's the quick filter's (the backend narrows the listing); on Jump it's the jump
 * buffer, HELD so the reset timeout and the reset keys leave it alone (`hold` in
 * `type-to-jump-state.svelte.ts`). Each press hands it across, so there's one pattern, and the
 * indicator, the MCP mirror, and the screen-reader count each keep reading their own controller.
 *
 * ## A press leaves the cursor alone
 *
 * Going to Jump, the backend keeps the cursor on its file through the unfiltering, and that file
 * matched. Going to Filter, it stays too when it matches. A jump only moves the cursor once the
 * pattern changes.
 *
 * A view with nothing to filter (the network views) opens on Jump and stays there; one with
 * neither (a load in progress, search results) ignores the key.
 */

import { tString } from '#lib/intl/messages.svelte.js'
import type { QuickFilterController } from './quick-filter-controller.svelte'
import type { TypeToJumpController } from './type-to-jump-controller.svelte'
import type { QuickFilterTyping } from './quick-filter-typing'

export type QuickFilterSessionKind = 'filter' | 'jump'

export interface QuickFilterSessionDeps {
  filter: Pick<
    QuickFilterController,
    'pattern' | 'canFilter' | 'isActive' | 'append' | 'backspace' | 'clear' | 'replace'
  >
  jump: Pick<
    TypeToJumpController,
    'buffer' | 'canMatch' | 'hold' | 'release' | 'handleJumpKeystroke' | 'backspaceHeld' | 'clearJumpState'
  >
  /** Says the kind through the pane's live region: on screen only the badge's word changes. */
  announce: (message: string) => void
  /** Plain typing just started a filter (never one ⌃S opened, which the person asked for). */
  onPlainFilterStarted: () => void
  /** The typing burst: every pattern edit renews it, and closing the session ends it. */
  typing: Pick<QuickFilterTyping, 'touch' | 'end'>
}

export interface QuickFilterSession {
  /** What the open session does with typing; `null` when plain typing follows the setting. */
  readonly kind: QuickFilterSessionKind | null
  /** ⌃S: opens the session, or switches its kind. */
  cycle: () => void
  append: (char: string) => void
  /** Drops the pattern's last character; on an empty badge, closes it. */
  backspace: () => void
  /** Esc or the ×: clears the pattern and closes the session. */
  clear: () => void
  /** Closes the session and drops a held pattern, ❌ no filter IPC: the pane is on new rows. */
  end: () => void
}

export function createQuickFilterSession(deps: QuickFilterSessionDeps): QuickFilterSession {
  const { filter, jump } = deps
  let kind = $state<QuickFilterSessionKind | null>(null)

  function say(next: QuickFilterSessionKind): void {
    deps.announce(
      tString(next === 'filter' ? 'fileExplorer.quickFilter.modeFilter' : 'fileExplorer.typeToJump.modeJump'),
    )
  }

  function open(): void {
    // A plain jump still running is what the person meant by "the current pattern".
    const typed = jump.buffer
    if (filter.canFilter()) {
      jump.clearJumpState()
      if (typed !== '' && !filter.isActive()) filter.replace(typed)
      kind = 'filter'
    } else if (jump.canMatch()) {
      jump.hold(typed)
      kind = 'jump'
    } else {
      return
    }
    say(kind)
  }

  function toJump(): void {
    jump.hold(filter.pattern)
    filter.clear()
    kind = 'jump'
    say(kind)
  }

  function toFilter(): void {
    if (!filter.canFilter()) return
    filter.replace(jump.buffer)
    jump.release()
    kind = 'filter'
    say(kind)
  }

  function end(): void {
    if (kind === 'jump') jump.release()
    kind = null
    deps.typing.end()
  }

  return {
    get kind() {
      return kind
    },
    cycle: () => {
      if (kind === null) open()
      else if (kind === 'filter') toJump()
      else toFilter()
    },
    append: (char) => {
      deps.typing.touch()
      if (kind === 'jump') {
        jump.handleJumpKeystroke(char)
        return
      }
      const starting = !filter.isActive()
      filter.append(char)
      if (kind === null && starting && filter.isActive()) deps.onPlainFilterStarted()
    },
    backspace: () => {
      deps.typing.touch()
      const pattern = kind === 'jump' ? jump.buffer : filter.pattern
      if (pattern === '') end()
      else if (kind === 'jump') jump.backspaceHeld()
      else filter.backspace()
    },
    clear: () => {
      filter.clear()
      end()
    },
    end,
  }
}
