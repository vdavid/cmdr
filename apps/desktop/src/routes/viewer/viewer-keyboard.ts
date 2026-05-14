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
