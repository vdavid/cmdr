interface NavigationActions {
  scrollByLines: (n: number) => void
  scrollByPages: (n: number) => void
  scrollToStart: () => void
  scrollToEnd: () => void
}

/** Maps Arrow / Page / Home / End keys to viewer scroll actions. Returns true if handled. */
export function handleNavigationKey(key: string, actions: NavigationActions): boolean {
  switch (key) {
    case 'ArrowUp':
      actions.scrollByLines(-1)
      return true
    case 'ArrowDown':
      actions.scrollByLines(1)
      return true
    case 'PageUp':
      actions.scrollByPages(-1)
      return true
    case 'PageDown':
      actions.scrollByPages(1)
      return true
    case 'Home':
      actions.scrollToStart()
      return true
    case 'End':
      actions.scrollToEnd()
      return true
    default:
      return false
  }
}

/** Handles single-letter toggles (word wrap on `W`). Returns true if handled. */
export function handleToggleKey(e: KeyboardEvent, toggleWordWrap: () => void): boolean {
  if (e.key.toLowerCase() === 'w' && !e.metaKey && !e.ctrlKey && !e.altKey) {
    toggleWordWrap()
    return true
  }
  return false
}

interface SearchToggleActions {
  toggleUseRegex: () => void
  toggleCaseSensitive: () => void
}

/** Handles the search-mode chords:
 *  - Cmd+Alt+R (or Ctrl+Alt+R on non-mac): toggle regex
 *  - Cmd+Alt+C (or Ctrl+Alt+C on non-mac): toggle case-sensitivity
 *
 *  Returns true if handled. Caller is responsible for `preventDefault`.
 *
 *  The chord is gated on both meta/ctrl AND alt so it can't collide with the
 *  in-input shortcuts (Cmd+A = select all, Cmd+C = copy). */
export function handleSearchToggleKey(e: KeyboardEvent, actions: SearchToggleActions): boolean {
  const modKey = e.metaKey || e.ctrlKey
  if (!modKey || !e.altKey) return false
  const key = e.key.toLowerCase()
  if (key === 'r') {
    actions.toggleUseRegex()
    return true
  }
  if (key === 'c') {
    actions.toggleCaseSensitive()
    return true
  }
  return false
}
