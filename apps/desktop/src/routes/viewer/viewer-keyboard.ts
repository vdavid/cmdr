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

/**
 * Handles the tail-mode toggle on the unmodified `F` key. Returns true if
 * handled. Gated on no modifier so it can't collide with `⌘F` (open search)
 * or other chords.
 */
export function handleTailToggleKey(e: KeyboardEvent, toggleTailMode: () => void): boolean {
  if (e.key.toLowerCase() === 'f' && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
    toggleTailMode()
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

interface KeyboardDeps {
  /** Total line count, or `null` in ByteSeek-no-index mode before an index exists. */
  getTotalLines: () => number | null
  /** Total byte count of the file (drives the ByteSeek-no-index ⌘A fallback). */
  getTotalBytes: () => number
  /** Reads the cached text of a line, or `undefined` if not cached. */
  getLineText: (line: number) => string | undefined
  selection: {
    /** Selects the whole file given the last line number and its length. */
    selectAll: (lastLine: number, lastLineLength: number) => void
  }
  scroll: NavigationActions
  search: {
    searchVisible: boolean
    searchStatus: string
    searchInputRef: HTMLInputElement | null | undefined
    openSearch: () => void
    closeSearch: () => void
    stopSearch: () => void
    findNext: () => void
    findPrev: () => void
    toggleUseRegex: () => void
    toggleCaseSensitive: () => void
  }
  copy: {
    busy: boolean
    cancelInFlight: () => Promise<void>
  }
  /** Whether a copy confirm dialog (10 to 100 MiB band) is currently showing. */
  isCopyConfirmOpen: () => boolean
  /** Whether the > 100 MiB copy refuse dialog is currently showing. */
  isCopyRefuseOpen: () => boolean
  /** Whether the in-app context menu is currently open. */
  isContextMenuOpen: () => boolean
  cancelCopyConfirm: () => void
  dismissCopyRefuse: () => void
  closeContextMenu: () => void
  /** Debug-logs the Escape press (search-visible / window-ready snapshot). */
  logEscape: () => void
  /** Runs the copy gesture (⌘C / context-menu Copy). */
  runCopy: () => void
  /** Toggles tail mode (unmodified `F`). */
  toggleTailMode: () => void
  /** Toggles word wrap (unmodified `W`). */
  toggleWordWrap: () => void
  /** Closes the viewer window (Escape with no other surface to consume it). */
  closeWindow: () => void
}

/**
 * Keyboard wiring for the viewer. Consolidates the page's keydown routing into one
 * place: the meta-shortcut handler, the Escape priority ladder, ⌘A select-all, and the
 * bare-key navigation/toggle dispatch. The page keeps a thin shim that binds
 * `handleKeyDown` to `<svelte:window on:keydown>` and passes the result of
 * `handleSelectAllShortcut` to the context menu's "Select all" action.
 *
 * Reads all reactive page state through getters/callbacks (the callback-based deps
 * pattern), so this stays a plain `.ts` module with no `$state` of its own.
 */
export function createViewerKeyboard(deps: KeyboardDeps) {
  function handleSelectAllShortcut(): void {
    const totalLines = deps.getTotalLines()
    if (totalLines !== null && totalLines > 0) {
      const lastLineText = deps.getLineText(totalLines - 1) ?? ''
      deps.selection.selectAll(totalLines, lastLineText.length)
      return
    }
    // ByteSeek-no-index ⌘A: we don't know `totalLines`. Use a sentinel that the
    // RangeEnd mapper translates to `RangeEnd::Eof` at the IPC boundary.
    if (deps.getTotalBytes() > 0) {
      deps.selection.selectAll(Number.MAX_SAFE_INTEGER, 0)
    }
  }

  function handleEscapeKey(): void {
    const { search } = deps
    deps.logEscape()
    if (!search.searchVisible) {
      deps.closeWindow()
      return
    }
    if (search.searchStatus === 'running') {
      search.stopSearch()
    } else {
      search.closeSearch()
    }
  }

  /**
   * Routes Escape to the right cancel surface in priority order: open context menu
   * (the menu owns its own Escape too, but we short-circuit here so the page's
   * `closeWindow()` path doesn't fire after the menu closes itself), then in-flight
   * copy read, then any open copy dialog, then the search bar logic.
   *
   * Returns `true` if Escape was consumed here.
   */
  function tryConsumeEscapeForCopy(): boolean {
    if (deps.isContextMenuOpen()) {
      deps.closeContextMenu()
      return true
    }
    if (deps.copy.busy) {
      void deps.copy.cancelInFlight()
      return true
    }
    if (deps.isCopyConfirmOpen()) {
      deps.cancelCopyConfirm()
      return true
    }
    if (deps.isCopyRefuseOpen()) {
      deps.dismissCopyRefuse()
      return true
    }
    return false
  }

  /**
   * Handles ⌘/Ctrl-prefixed shortcuts inside the viewer. Returns `true` if the key
   * was consumed; the caller falls through to other handlers when it returns `false`.
   * Defers to the browser's native ⌘A / ⌘C when the search input is focused.
   */
  function handleModifierShortcut(e: KeyboardEvent, searchInputFocused: boolean): boolean {
    if (searchInputFocused) {
      // Only ⌘F here; ⌘A / ⌘C go to the input's native handler.
      if (e.key === 'f') {
        e.preventDefault()
        deps.search.openSearch()
        return true
      }
      return false
    }
    if (e.key === 'a') {
      e.preventDefault()
      handleSelectAllShortcut()
      return true
    }
    if (e.key === 'c') {
      e.preventDefault()
      deps.runCopy()
      return true
    }
    if (e.key === 'f') {
      e.preventDefault()
      deps.search.openSearch()
      return true
    }
    return false
  }

  /**
   * Routes unmodified single-letter and navigation keys to their handler.
   * Split out from `handleKeyDown` to keep the latter's cyclomatic
   * complexity below the project lint threshold.
   */
  function handleBareKey(e: KeyboardEvent): boolean {
    return (
      handleTailToggleKey(e, deps.toggleTailMode) ||
      handleToggleKey(e, deps.toggleWordWrap) ||
      handleNavigationKey(e.key, deps.scroll)
    )
  }

  function handleKeyDown(e: KeyboardEvent): void {
    const { search } = deps
    const searchInputFocused = search.searchVisible && document.activeElement === search.searchInputRef

    // Search-mode chords (⌘⌥R for regex, ⌘⌥C for case) work whenever the search
    // bar is visible, even if the input has focus. Checked before the generic
    // modifier-shortcut handler so the alt-bearing chord wins.
    if (
      search.searchVisible &&
      handleSearchToggleKey(e, {
        toggleUseRegex: search.toggleUseRegex,
        toggleCaseSensitive: search.toggleCaseSensitive,
      })
    ) {
      e.preventDefault()
      return
    }

    if ((e.metaKey || e.ctrlKey) && handleModifierShortcut(e, searchInputFocused)) return

    if (e.key === 'Escape') {
      e.preventDefault()
      if (tryConsumeEscapeForCopy()) return
      handleEscapeKey()
      return
    }

    if (e.key === 'Enter' && search.searchVisible) {
      e.preventDefault()
      if (e.shiftKey) search.findPrev()
      else search.findNext()
      return
    }

    if (searchInputFocused) return

    if (handleBareKey(e)) e.preventDefault()
  }

  return { handleKeyDown, handleSelectAllShortcut }
}
