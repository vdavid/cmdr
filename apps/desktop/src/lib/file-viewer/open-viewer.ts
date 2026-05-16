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

  new WebviewWindow(label, {
    url: `/viewer?path=${encodedPath}`,
    title: decorateChildWindowTitle(filePath.split('/').pop() ?? 'Viewer'),
    width: 800,
    height: 600,
    minWidth: 400,
    minHeight: 300,
    resizable: true,
    minimizable: true,
    maximizable: true,
    closable: true,
    focus: !isE2e,
  })
}
