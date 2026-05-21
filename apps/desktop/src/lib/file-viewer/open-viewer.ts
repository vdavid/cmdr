import { cascadeFromMain, clampToMonitor, nearestMonitor, readMainRect, readMonitors } from '$lib/window-positioning'

const VIEWER_WIDTH = 800
const VIEWER_HEIGHT = 600

/**
 * Monotonic per-session counter for cascading viewer windows. Reset on app
 * launch (module-scope). Wraps inside `cascadeFromMain` so a long session
 * doesn't march viewers off the screen.
 */
let cascadeIndex = 0

/** Opens a file viewer window for the given file path. Multiple viewers can be open at once. */
export async function openFileViewer(filePath: string): Promise<void> {
  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
  const { decorateChildWindowTitle, getAppMode } = await import('$lib/app-mode')

  // Use a unique label per viewer instance (timestamp-based)
  const label = `viewer-${String(Date.now())}`
  const encodedPath = encodeURIComponent(filePath)

  // E2E suites open viewer windows repeatedly; stealing OS focus each time
  // makes the host machine unusable while tests run. The plugin reaches the
  // webview over a Unix socket, so it doesn't need OS focus to drive the DOM.
  const isE2e = getAppMode() === 'e2e'

  // Cascade from main's top-left so multiple viewers don't pile on top of each
  // other. Falls back to Tauri's `center: true` if main isn't open.
  const [main, monitors] = await Promise.all([readMainRect(), readMonitors()])
  const size = { width: VIEWER_WIDTH, height: VIEWER_HEIGHT }
  let rect = main ? cascadeFromMain(main, size, cascadeIndex++) : null
  if (rect) {
    const monitor = nearestMonitor(rect, monitors)
    if (monitor) rect = clampToMonitor(rect, monitor)
  }

  new WebviewWindow(label, {
    url: `/viewer?path=${encodedPath}`,
    title: decorateChildWindowTitle(filePath.split('/').pop() ?? 'Viewer'),
    width: VIEWER_WIDTH,
    height: VIEWER_HEIGHT,
    minWidth: 400,
    minHeight: 300,
    ...(rect ? { x: rect.x, y: rect.y } : { center: true }),
    resizable: true,
    minimizable: true,
    maximizable: true,
    closable: true,
    focus: !isE2e,
  })
}
