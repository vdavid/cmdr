/**
 * Whether the person is still typing into the quick filter: the TYPING BURST.
 *
 * Inside a burst, every printable key extends the pattern, so `my file*` types as one
 * name. Once the burst ends, a key bound to a command (Space, `-`, `*`, …) runs that
 * command again, so Space selects the files the filter left. The burst ends when the
 * gap since the last pattern edit passes `fileExplorer.quickFilter.typingWindow`, or at
 * once when the cursor moves (a navigation key, a row click) or the filter closes.
 *
 * Pure apart from the clock, which tests inject.
 */

export interface QuickFilterTyping {
  /** A pattern edit just happened (a typed character or a Backspace). */
  touch: () => void
  /** The burst is over now: the cursor moved or the filter closed. */
  end: () => void
  /** Whether a key pressed now still counts as typing. */
  isLive: () => boolean
}

export interface QuickFilterTypingDeps {
  /** The window, read live so a Settings change counts on the next key. */
  getWindowMs: () => number
  now?: () => number
}

export function createQuickFilterTyping(deps: QuickFilterTypingDeps): QuickFilterTyping {
  const now = deps.now ?? (() => performance.now())
  let lastEditAt: number | null = null
  return {
    touch: () => {
      lastEditAt = now()
    },
    end: () => {
      lastEditAt = null
    },
    isLive: () => lastEditAt !== null && now() - lastEditAt < deps.getWindowMs(),
  }
}
