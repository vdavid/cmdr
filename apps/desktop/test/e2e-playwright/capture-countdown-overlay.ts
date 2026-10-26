/**
 * The hands-off countdown both capture specs show inside the Cmdr window right before
 * they start driving it: a solid orange layer over the whole window, a big "3, 2, 1",
 * and the run's expected duration.
 *
 * Whoever started the run is usually still typing somewhere; once Cmdr takes the front,
 * a stray letter, Tab, Enter, or Space would land in a pane and corrupt the staged
 * state. So while the overlay is up, capture-phase listeners on `window` swallow
 * keyboard and pointer input. Menu accelerators the OS handles natively can still slip
 * through; that's accepted.
 *
 * Test-side DOM injection only, no app code. The orchestrator decides whether there is
 * a countdown at all and what it says (`scripts/capture-countdown.ts`), passing it as
 * `CMDR_CAPTURE_COUNTDOWN_S` / `CMDR_CAPTURE_COUNTDOWN_LINE`; a lane run of the same
 * spec sets neither and never counts down.
 *
 * ❗ It can't strand an orange window or dead input: `runCaptureCountdown` removes it in
 * a `finally`, and the page removes it on its own a few seconds past the countdown even
 * if the test process is gone.
 */

import { writeFileSync } from 'node:fs'
import type { TauriPage } from '@srsholmes/tauri-playwright'

export const OVERLAY_ID = 'cmdr-capture-countdown'

/** How long past the countdown the page keeps the overlay before removing it by itself. */
// eslint-disable-next-line cmdr/no-raw-wait-budget -- a self-destruct deadline for a human-facing overlay, not a wait on the machine: scaling it would only keep an orange window up longer
const HARD_TIMEOUT_GRACE_S = 5

const SWALLOWED_EVENTS = [
  'keydown',
  'keyup',
  'keypress',
  'pointerdown',
  'pointerup',
  'mousedown',
  'mouseup',
  'click',
  'dblclick',
  'contextmenu',
  'wheel',
]

export interface Countdown {
  seconds: number
  /** The estimate line, already worded by the orchestrator. */
  line: string
  /** Where to stamp the moment the countdown ended; the orchestrator's estimate times from it. */
  endedFile?: string
}

/** The countdown the orchestrator asked for, or null when this run isn't a capture that wants one. */
export function countdownFromEnv(env: Record<string, string | undefined>): Countdown | null {
  const seconds = Number(env.CMDR_CAPTURE_COUNTDOWN_S)
  const line = env.CMDR_CAPTURE_COUNTDOWN_LINE
  if (!Number.isInteger(seconds) || seconds <= 0 || line === undefined) return null
  return { seconds, line, endedFile: env.CMDR_CAPTURE_COUNTDOWN_ENDED_FILE }
}

/** Installs the overlay and the input swallowing; replaces an earlier one. One page expression. */
export function installOverlayScript({ seconds, line }: Countdown): string {
  return `(function() {
    var w = window;
    if (w.__cmdrCaptureCountdown) w.__cmdrCaptureCountdown.remove();
    var types = ${JSON.stringify(SWALLOWED_EVENTS)};
    var swallow = function(e) { e.preventDefault(); e.stopImmediatePropagation(); };

    var el = document.createElement('div');
    el.id = ${JSON.stringify(OVERLAY_ID)};
    el.style.cssText = 'position:fixed;inset:0;z-index:2147483647;background:#ff7300;color:#1a1a1a;' +
      'display:flex;flex-direction:column;align-items:center;justify-content:center;gap:2vh;' +
      'font-family:system-ui,-apple-system,sans-serif;cursor:default;user-select:none;-webkit-user-select:none;';
    var count = document.createElement('div');
    count.setAttribute('data-countdown-count', '');
    count.style.cssText = 'font-size:clamp(96px,45vh,360px);font-weight:700;line-height:1;font-variant-numeric:tabular-nums;';
    count.textContent = ${JSON.stringify(String(seconds))};
    var text = document.createElement('div');
    text.style.cssText = 'font-size:clamp(16px,4vh,32px);font-weight:500;';
    text.textContent = ${JSON.stringify(line)};
    el.appendChild(count);
    el.appendChild(text);

    var timer;
    var api = {
      set: function(n) { count.textContent = String(n); },
      remove: function() {
        clearTimeout(timer);
        types.forEach(function(t) { w.removeEventListener(t, swallow, true); });
        el.remove();
        if (w.__cmdrCaptureCountdown === api) delete w.__cmdrCaptureCountdown;
      },
    };
    types.forEach(function(t) { w.addEventListener(t, swallow, { capture: true, passive: false }); });
    timer = setTimeout(api.remove, ${String((seconds + HARD_TIMEOUT_GRACE_S) * 1000)});
    w.__cmdrCaptureCountdown = api;
    (document.body || document.documentElement).appendChild(el);
    return true;
  })()`
}

export function setCountScript(n: number): string {
  return `(function() { var c = window.__cmdrCaptureCountdown; if (c) c.set(${String(n)}); return true; })()`
}

export function removeOverlayScript(): string {
  return `(function() { var c = window.__cmdrCaptureCountdown; if (c) c.remove(); return true; })()`
}

/**
 * Waits for the webview to composite a frame without the overlay, so the first shutter
 * can't catch it. Two animation frames, racing a short timeout because
 * `requestAnimationFrame` pauses on a window that isn't foreground.
 */
async function waitForPaint(page: TauriPage): Promise<void> {
  await page.evaluate(`new Promise(function(resolve) {
    var done = false;
    var finish = function() { if (!done) { done = true; resolve(true); } };
    requestAnimationFrame(function() { requestAnimationFrame(finish); });
    setTimeout(finish, 500);
  })`)
}

let shown = false

/**
 * The spec entry point: when the orchestrator asked for a countdown, fronts the window
 * and shows it, once per run. A no-op otherwise, which is every lane run.
 */
export async function countDownOnceIfAsked(page: TauriPage, bringToFront: () => unknown): Promise<void> {
  const countdown = countdownFromEnv(process.env)
  if (countdown === null || shown) return
  shown = true
  await bringToFront()
  await runCaptureCountdown(page, countdown)
  // The orchestrator times the run from here, so nothing before the countdown counts.
  if (countdown.endedFile !== undefined) writeFileSync(countdown.endedFile, String(Date.now()))
}

/**
 * Shows the countdown, one second per number, then takes it away and waits for a paint.
 * The caller brings the window to the front first.
 */
export async function runCaptureCountdown(page: TauriPage, countdown: Countdown): Promise<void> {
  await page.evaluate(installOverlayScript(countdown))
  try {
    for (let n = countdown.seconds; n > 0; n--) {
      await page.evaluate(setCountScript(n))
      // The one second each number stays up: a timed display for a human, not a readiness wait.
      await new Promise((resolve) => setTimeout(resolve, 1000))
    }
  } finally {
    await page.evaluate(removeOverlayScript())
    await waitForPaint(page)
  }
}
