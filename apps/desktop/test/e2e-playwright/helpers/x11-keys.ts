/**
 * Real X key events for the Linux lane, through the packaged `xdotool` (XTest, so the X server
 * and GTK see a device keypress). `pressKey` builds a synthetic DOM event inside the webview,
 * which GTK never sees: anything GTK takes before the page (the bar's `Alt+<letter>` mnemonics,
 * the F-keys) needs this. Linux only; `xdotool` comes with the lane's base image.
 *
 * The Xvfb display has no window manager, so nothing gives a window focus by itself:
 * `focusMainWindow` sets the X input focus and parks the pointer on the biggest visible
 * top-level window (the main window), which is where XTest key events land.
 */
import { execFileSync } from 'node:child_process'

function xdotool(...args: string[]): string {
  return execFileSync('xdotool', args, { encoding: 'utf8' })
}

/** The visible direct children of the root window, by X id. An open GTK menu is one of these. */
export function visibleTopLevelWindows(): string[] {
  try {
    // `--name ''` matches every window; xdotool exits 1 when nothing matches.
    return xdotool('search', '--onlyvisible', '--maxdepth', '1', '--name', '')
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line !== '')
  } catch {
    return []
  }
}

function size(windowId: string): { width: number; height: number } {
  const shell = xdotool('getwindowgeometry', '--shell', windowId)
  return {
    width: Number(/WIDTH=(\d+)/.exec(shell)?.[1] ?? 0),
    height: Number(/HEIGHT=(\d+)/.exec(shell)?.[1] ?? 0),
  }
}

function area(windowId: string): number {
  const { width, height } = size(windowId)
  return width * height
}

/** Gives the main window the X input focus and the pointer, so real keys reach it. */
export function focusMainWindow(): void {
  const windows = visibleTopLevelWindows()
  if (windows.length === 0) throw new Error('no visible X window to focus')
  const main = windows.reduce((best, id) => (area(id) > area(best) ? id : best))
  xdotool('windowfocus', '--sync', main)
  xdotool('mousemove', '--window', main, '400', '300')
}

/**
 * Gives one X window (an id from `visibleTopLevelWindows`) the input focus and clicks its middle,
 * so GTK's own focus lands on the webview: a window that opened under a bar starts with no focused
 * widget, and real keys go nowhere until the page is clicked, as a user would.
 */
export function focusWindow(windowId: string): void {
  const { width, height } = size(windowId)
  xdotool('windowfocus', '--sync', windowId)
  xdotool('mousemove', '--window', windowId, String(Math.floor(width / 2)), String(Math.floor(height / 2)))
  xdotool('click', '1')
}

/** Presses one chord as a real keyboard would, in xdotool's spelling (`alt+f`, `Escape`). */
export function pressRealKey(chord: string): void {
  xdotool('key', '--clearmodifiers', chord)
}
