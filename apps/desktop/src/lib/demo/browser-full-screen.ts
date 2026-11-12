/**
 * The browser's full screen, the closest a page gets to a window's. The main window's zoom light
 * (`traffic-lights.ts`) and the app's own full-screen calls (`webview.ts` answers the window's
 * `set_fullscreen` / `is_fullscreen`) both come here, so the demo makes no Tauri window call.
 */

/** Whether this page can enter full screen at all: a frame needs its embedder's `allow="fullscreen"`. */
export function canFullScreen(): boolean {
  return document.fullscreenEnabled && typeof document.documentElement.requestFullscreen === 'function'
}

/** Whether the page is in the browser's full screen. */
export function isBrowserFullScreen(): boolean {
  return (document.fullscreenElement ?? null) !== null
}

/**
 * Puts the page into the browser's full screen or takes it out. Refused quietly where the page
 * can't (a frame that doesn't allow it, no user gesture behind the call), as the green button is.
 * Resolves either way.
 */
export async function setBrowserFullScreen(on: boolean): Promise<void> {
  if (!canFullScreen() || on === isBrowserFullScreen()) return
  const done = on
    ? // eslint-disable-next-line cmdr/webkit-floor-dom -- `canFullScreen()` found `requestFullscreen`
      document.documentElement.requestFullscreen()
    : // eslint-disable-next-line cmdr/webkit-floor-dom -- `canFullScreen()` found `requestFullscreen`, which ships with `exitFullscreen`
      document.exitFullscreen()
  await done.catch(() => undefined)
}
