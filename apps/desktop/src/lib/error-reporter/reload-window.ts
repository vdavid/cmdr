/**
 * Reloads this window's webview from scratch: the last resort of a crashed window, after
 * re-arming its boundary didn't help. Its own module so a test can stand in for it (jsdom's
 * `location.reload` can't be spied on).
 */
export function reloadWindow(): void {
  location.reload()
}
