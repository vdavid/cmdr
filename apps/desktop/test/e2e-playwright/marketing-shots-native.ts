/**
 * The macOS half of the marketing capture: taking the front position, finding a
 * window's CGWindowID, and working the `screencapture` shutter.
 *
 * Why this exists at all: the masters are the window exactly as macOS draws it, minus
 * the shadow (`-o`), with the system's rounded corners on transparency. Every consumer
 * adds its own shadow, or none: a baked-in one is invisible on a dark page and only
 * shrinks the window in a README or a listing.
 *
 * The pure judgement calls live in `marketing-shots-frame.ts`; everything here shells
 * out and is proven by real runs.
 */

import { waitBudget } from './wait-budget.js'
import { execFileSync } from 'node:child_process'
import type { NativeWindow, WindowSize } from './marketing-shots-frame.js'

/** How long any one shell-out gets before we call it wedged. */
const NATIVE_TIMEOUT_MS = waitBudget(15000)

/**
 * `CGWindowListCopyWindowInfo` options, spelled as numbers on purpose:
 * `$.kCGWindowListOptionOnScreenOnly` and friends are undefined inside JXA, so the
 * named constants silently pass `NaN` and the call returns nothing.
 */
const ON_SCREEN_ONLY = 1
const EXCLUDE_DESKTOP_ELEMENTS = 16

/**
 * JXA that dumps one process's on-screen windows as JSON.
 *
 * ❗ `ObjC.bindFunction` is required, not decoration. `CGWindowListCopyWindowInfo` is
 * missing from the CoreGraphics bridge support JXA loads, so `$.CGWindowList…` resolves
 * to an unusable stub and `ObjC.deepUnwrap` hands back a function instead of an array.
 * Binding the symbol by hand is what makes the call real.
 *
 * Bounds are in POINTS, so a retina window that photographs 2284x1410 lists as
 * 1142x705.
 */
const WINDOW_DUMP_JXA = `
ObjC.import('Foundation')
ObjC.bindFunction('CGWindowListCopyWindowInfo', ['id', ['int', 'unsigned int']])
const pid = Number($.NSProcessInfo.processInfo.environment.objectForKey('CMDR_SHOTS_TARGET_PID').js)
const all = ObjC.deepUnwrap($.CGWindowListCopyWindowInfo(${String(ON_SCREEN_ONLY | EXCLUDE_DESKTOP_ELEMENTS)}, 0))
JSON.stringify(
  all
    .filter((w) => w.kCGWindowOwnerPID === pid)
    .map((w) => ({
      id: w.kCGWindowNumber,
      layer: w.kCGWindowLayer,
      x: Math.round(w.kCGWindowBounds.X),
      y: Math.round(w.kCGWindowBounds.Y),
      width: Math.round(w.kCGWindowBounds.Width),
      height: Math.round(w.kCGWindowBounds.Height),
    })),
)
`

/** Every on-screen window belonging to `pid`, in points. */
export function listAppWindows(pid: number): NativeWindow[] {
  const raw = runOsascript(['-l', 'JavaScript', '-e', WINDOW_DUMP_JXA], {
    CMDR_SHOTS_TARGET_PID: String(pid),
  })
  const parsed: unknown = JSON.parse(raw)
  if (!Array.isArray(parsed)) {
    throw new Error(`Expected a JSON array of windows from JXA, got: ${raw.slice(0, 200)}`)
  }
  return parsed as NativeWindow[]
}

/**
 * Makes `pid`'s app the active one, so macOS composites its window AND draws it as the
 * key window (coloured traffic lights).
 *
 * This is deliberately done from OUTSIDE the app. The app's own
 * `plugin:window|set_focus` cannot take the front position from another app when it
 * runs as a raw binary rather than a bundle through LaunchServices, which is exactly
 * how a capture run goes blank with nobody touching the laptop.
 */
export function focusApp(pid: number): void {
  runOsascript([
    '-e',
    `tell application "System Events" to set frontmost of (first process whose unix id is ${String(pid)}) to true`,
  ])
}

/** The unix pid of the frontmost app, as System Events sees it. */
export function frontmostPid(): number {
  return Number(
    runOsascript([
      '-e',
      'tell application "System Events" to get unix id of first process whose frontmost is true',
    ]).trim(),
  )
}

/**
 * Photographs window `windowId` into `path`, WITHOUT its shadow (`-o`): the window
 * alone, transparent only in its rounded corners. Every consumer adds its own shadow,
 * or none.
 */
export function captureWindow(windowId: number, path: string): void {
  try {
    execFileSync('screencapture', ['-x', '-o', '-t', 'png', '-l', String(windowId), path], {
      timeout: NATIVE_TIMEOUT_MS,
    })
  } catch (err) {
    throw new Error(
      `\`screencapture\` could not photograph window ${String(windowId)}: ${messageOf(err)}. ` +
        'Grant Screen Recording permission to the terminal running this, then re-run.',
      { cause: err },
    )
  }
}

/** The one window of `pid` measuring `size` points, raising a message a reader can act on. */
export function requireWindowId(pid: number, size: WindowSize, what: string): number {
  const windows = listAppWindows(pid)
  const ordinary = windows.filter((candidate) => candidate.layer === 0)
  const match = ordinary.find((candidate) => candidate.width === size.width && candidate.height === size.height)
  if (match === undefined) {
    const seen = ordinary.map((w) => `${String(w.width)}x${String(w.height)}`).join(', ') || 'none'
    throw new Error(
      `No ${what} window of ${String(size.width)}x${String(size.height)} points belongs to pid ${String(pid)}. ` +
        `On-screen windows: ${seen}. A window that is minimized or on another Space is not listed.`,
    )
  }
  return match.id
}

function runOsascript(args: string[], extraEnv: Record<string, string> = {}): string {
  try {
    return execFileSync('osascript', args, {
      encoding: 'utf8',
      timeout: NATIVE_TIMEOUT_MS,
      env: { ...process.env, ...extraEnv },
    })
  } catch (err) {
    throw new Error(
      `\`osascript\` failed: ${messageOf(err)}. ` +
        'If this mentions "not allowed assistive access", grant Accessibility permission to the terminal running ' +
        'this: the capture asks System Events to bring the app to the front, since macOS composites only the ' +
        'frontmost window and colours the traffic lights only for the key window.',
      { cause: err },
    )
  }
}

function messageOf(err: unknown): string {
  if (err instanceof Error) {
    const stderr = (err as { stderr?: Buffer | string }).stderr
    const detail = typeof stderr === 'string' ? stderr : stderr?.toString('utf8')
    return detail !== undefined && detail.trim() !== '' ? detail.trim() : err.message
  }
  return String(err)
}
