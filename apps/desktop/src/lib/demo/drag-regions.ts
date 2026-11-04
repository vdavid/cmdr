/**
 * The demo's stand-in for the script Tauri injects into every webview (`tauri/src/window/scripts/
 * drag.js`): a press on a `data-tauri-drag-region` asks for `plugin:window|start_dragging`, and a
 * double click released where it was pressed asks for `internal_toggle_maximize`, macOS's title-bar
 * zoom. The rules are Tauri's, so the app's drag strips (Settings' and Keyboard shortcuts' rows under
 * the lights) behave as on desktop. The one addition: the press's screen position, which a page
 * needs to move a panel and the OS doesn't (`windows.ts`). See `DETAILS.md` § Secondary windows.
 */

type Invoke = (cmd: string, payload: Record<string, unknown>) => unknown

const DRAG_REGION_ATTR = 'data-tauri-drag-region'
const CLICKABLE_TAGS = new Set(['A', 'BUTTON', 'INPUT', 'SELECT', 'TEXTAREA', 'LABEL', 'SUMMARY'])
const INTERACTIVE_ROLES = new Set(['button', 'link', 'menuitem', 'tab', 'checkbox', 'radio', 'switch', 'option'])

function isClickable(element: HTMLElement): boolean {
  return (
    CLICKABLE_TAGS.has(element.tagName) ||
    (element.hasAttribute('contenteditable') && element.getAttribute('contenteditable') !== 'false') ||
    (element.hasAttribute('tabindex') && element.getAttribute('tabindex') !== '-1') ||
    INTERACTIVE_ROLES.has(element.getAttribute('role') ?? '')
  )
}

/** Tauri's walk: a bare region is itself only, `deep` its subtree, `false` or a control in between blocks. */
function isDragRegion(path: EventTarget[]): boolean {
  for (const element of path) {
    if (!(element instanceof HTMLElement)) continue
    const attr = element.getAttribute(DRAG_REGION_ATTR)
    if (attr === null) {
      if (isClickable(element)) return false
      continue
    }
    if (attr === 'false') return false
    if (attr === 'deep') return true
    if (attr === '' || attr === 'true') return element === path[0]
  }
  return false
}

/** Listens on this frame's document. Returns the teardown. */
export function installDragRegions(invoke: Invoke): () => void {
  // Where a double click went down: macOS zooms on its release, and only if the mouse stayed put.
  let doubleAt: { x: number; y: number } | null = null

  function onMouseDown(event: MouseEvent): void {
    if (event.button !== 0 || (event.detail !== 1 && event.detail !== 2)) return
    if (!isDragRegion(event.composedPath())) return
    if (event.detail === 2) {
      doubleAt = { x: event.clientX, y: event.clientY }
      return
    }
    event.preventDefault()
    event.stopImmediatePropagation()
    // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- the stand-in for Tauri's own injected script, which sends this plugin command raw; no binding exists for it
    void invoke('plugin:window|start_dragging', { screenX: event.screenX, screenY: event.screenY })
  }

  function onMouseUp(event: MouseEvent): void {
    const at = doubleAt
    doubleAt = null
    if (event.button !== 0 || event.detail !== 2 || !at) return
    if (event.clientX !== at.x || event.clientY !== at.y || !isDragRegion(event.composedPath())) return
    // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- the stand-in for Tauri's own injected script, which sends this plugin command raw; no binding exists for it
    void invoke('plugin:window|internal_toggle_maximize', {})
  }

  document.addEventListener('mousedown', onMouseDown)
  document.addEventListener('mouseup', onMouseUp)
  return () => {
    document.removeEventListener('mousedown', onMouseDown)
    document.removeEventListener('mouseup', onMouseUp)
  }
}
