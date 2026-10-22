/**
 * Reloads this window's webview from scratch: the last resort of a crashed window, after
 * re-arming its boundary didn't help. Its own module so a test can stand in for it (jsdom's
 * `location.reload` can't be spied on), and it takes the location to reload so its own test can
 * hand it a stand-in.
 */
export function reloadWindow(target: Pick<Location, 'reload'> = location): void {
  target.reload()
}
