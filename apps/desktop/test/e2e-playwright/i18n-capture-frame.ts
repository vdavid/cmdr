/**
 * What has to be true of the WINDOW and its FRAMING before the i18n capture
 * harness presses the shutter, and how the shot gets framed afterwards.
 *
 * Four jobs, all measured off live DOM state so nothing here guesses a number:
 *
 *  - **Crop geometry** (`measureCropGeometry`): an element's rect in IMAGE
 *    pixels, plus what the whole window's image SHOULD measure, so `shoot` can
 *    assert the 1:1 mapping before it trusts the rect. The mapping holds because
 *    Cmdr draws its own title bar inside the webview (the traffic lights sit on
 *    the `.title-bar` element), so the webview covers the window with no chrome
 *    offset. That's an assertion here, not an assumption.
 *  - **Fitting** (`fitWindowToContent`): grow a window until the surface's
 *    content stops scrolling, so a dialog isn't photographed cut off at the
 *    bottom. Translators judge text at its real size, so the window grows first;
 *    only when the display runs out does the UI zoom come down.
 *  - **Toast hygiene** (`strayToasts` / `clearStrayToasts`): a toast nothing
 *    staged (the virtual MTP device announcing itself) lands on its own
 *    schedule, so it can appear DURING an unrelated surface's shot and be
 *    photographed mid-slide-in, translucent, over the dialog. Checked as a
 *    precondition AND a postcondition; the postcondition is what makes it
 *    airtight.
 *  - **Clip scanning** (`scanForClipping`): the overflow pass's best-effort DOM
 *    sweep for text its own box cuts off.
 *
 * Kept out of `i18n-capture-helpers.ts` for the file-length budget, and free of
 * the Playwright test runtime on purpose, so its pure parts (`straysIn`,
 * `selectorList`) are unit-testable without a running app.
 */

import { DEFAULT_UI_ZOOM, isOverflowPass } from './i18n-capture-config.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import type { CropRect } from './i18n-capture-png.js'

/**
 * Breathing room around a cropped element, in CSS px. A dialog's drop shadow and
 * the pane behind it are part of reading the shot as a dialog rather than a
 * floating rectangle, and shaving a shadow mid-blur reads as a hard edge.
 *
 * ❗ Padding wider than the gap between an element and its neighbors pulls those
 * neighbors into frame. That's fine for a dialog floating over the file panes,
 * and wrong inside a grid: `CROP_PADDING_TIGHT_CSS_PX` is for elements packed
 * next to each other (the indexing tiles sit in a `--spacing-lg` / 16px grid
 * gap, so 24px of padding reached 8px into the tile beside it).
 */
export const CROP_PADDING_CSS_PX = 24
export const CROP_PADDING_TIGHT_CSS_PX = 8

/**
 * Resolves a selector LIST to the first entry that matches, page-side. A surface
 * names its frame more than one way because the same concept has more than one
 * shape: a `ModalDialog`'s `data-dialog-id` sits on the full-window OVERLAY with
 * the frame inside it (`.modal-dialog`), while the onboarding wizard puts the
 * same attribute on the panel itself. `document.querySelector('a, b')` can't
 * express "prefer a", since it returns whichever comes first in the document.
 */
function firstMatchExpression(selectors: string[]): string {
  return `(function(){
    var list = ${JSON.stringify(selectors)};
    for (var i = 0; i < list.length; i++) {
      var found = document.querySelector(list[i]);
      if (found) return found;
    }
    return null;
  })()`
}

/** One or more selectors for a surface's own frame, tried in order. */
export type FrameSelector = string | string[]

/** Normalizes the ergonomic single-selector form to the list the page-side code takes. */
export function selectorList(selector: FrameSelector): string[] {
  return typeof selector === 'string' ? [selector] : selector
}

/** An element's crop window, plus what the full-window image must measure for it to be valid. */
export interface CropGeometry {
  /** The padded element rect, in image pixels. */
  rect: CropRect
  /** `innerWidth * devicePixelRatio`: the width the window's PNG must have. */
  expectedImageWidth: number
  /** `innerHeight * devicePixelRatio`: the height the window's PNG must have. */
  expectedImageHeight: number
}

/**
 * Measures `selector`'s padded bounding rect in image pixels, or null when the
 * element isn't there or has no box. The caller compares `expectedImage*` against
 * the PNG it actually got and refuses to crop on a mismatch: a rect derived from
 * layout coordinates is only meaningful while the webview and the image share an
 * origin and a scale.
 */
export async function measureCropGeometry(
  page: TauriPage,
  selector: FrameSelector,
  paddingCssPx: number = CROP_PADDING_CSS_PX,
): Promise<CropGeometry | null> {
  return page.evaluate<CropGeometry | null>(`(function(){
    var el = ${firstMatchExpression(selectorList(selector))};
    if (!el) return null;
    var r = el.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) return null;
    var d = window.devicePixelRatio || 1;
    var p = ${String(paddingCssPx)};
    return {
      rect: {
        left: (r.left - p) * d,
        top: (r.top - p) * d,
        width: (r.width + 2 * p) * d,
        height: (r.height + 2 * p) * d
      },
      expectedImageWidth: Math.round(window.innerWidth * d),
      expectedImageHeight: Math.round(window.innerHeight * d)
    };
  })()`)
}

/** What one fit measurement found under a surface's root element. */
interface FitMeasurement {
  /** Logical px of height the window needs to gain for the content to fit. */
  need: number
  innerWidth: number
  innerHeight: number
  /** The tallest this window can usefully be on this display, in logical px. */
  maxHeight: number
  /** Boxes clipping content with no way to scroll it into view: a real UI bug. */
  unreachable: string[]
}

/**
 * Measures how much taller `selector`'s window would have to be for the surface's
 * content to stop scrolling.
 *
 * Two contributions, whichever is larger: the deepest SCROLL container under the
 * root (`scrollHeight - clientHeight` where `overflow-y` is `auto`/`scroll`), and
 * how far the root's own box sits outside the viewport. Anything clipping content
 * with `hidden`/`clip` is reported separately as `unreachable`: growing the window
 * won't reveal it, because nothing can scroll it into view.
 */
async function measureFit(page: TauriPage, selector: FrameSelector): Promise<FitMeasurement | null> {
  return page.evaluate<FitMeasurement | null>(`(function(){
    var root = ${firstMatchExpression(selectorList(selector))};
    if (!root) return null;
    var need = 0;
    var unreachable = [];
    var nodes = [root].concat(Array.prototype.slice.call(root.querySelectorAll('*')));
    for (var i = 0; i < nodes.length; i++) {
      var el = nodes[i];
      var over = el.scrollHeight - el.clientHeight;
      if (over <= 1) continue;
      var s = getComputedStyle(el);
      if (s.overflowY === 'auto' || s.overflowY === 'scroll') {
        if (over > need) need = over;
        continue;
      }
      if (s.overflowY === 'hidden' || s.overflowY === 'clip') {
        var cls = (typeof el.className === 'string') ? el.className : '';
        // Visually-hidden accessibility nodes clip BY DESIGN: the standard
        // 'sr-only' pattern collapses the box to a 1px clip rect, so every one of
        // them looks like unreachable content and none of them is. Reporting them
        // buries the real finding this scan exists for. Same exclusion the
        // overflow pass's clip scan makes.
        if (/\\bsr-only\\b/.test(cls) || el.id === 'svelte-announcer') continue;
        if (el.clientWidth <= 1 || el.clientHeight <= 1) continue;
        var sel = el.tagName.toLowerCase();
        if (el.id) sel += '#' + el.id;
        var short = cls.trim().split(/\\s+/).slice(0, 2).join('.');
        if (short) sel += '.' + short;
        unreachable.push(sel + ' (+' + String(Math.round(over)) + 'px)');
      }
    }
    var r = root.getBoundingClientRect();
    var outside = Math.max(0, Math.ceil(r.bottom - window.innerHeight)) + Math.max(0, Math.ceil(-r.top));
    if (outside > need) need = outside;
    // The display is the hard limit. Two ceilings: the screen's work area, and
    // where this window actually sits (a window near the bottom would otherwise
    // grow off the screen, and only the on-screen part is worth photographing).
    var byScreen = window.screen.availHeight - 16;
    var byPosition = window.screen.height - window.screenY - 8;
    return {
      need: Math.ceil(need),
      innerWidth: window.innerWidth,
      innerHeight: window.innerHeight,
      maxHeight: Math.max(200, Math.min(byScreen, byPosition)),
      unreachable: unreachable
    };
  })()`)
}

/** Sets a window's LOGICAL size through the same IPC shape `setSize` produces. */
async function setWindowSize(page: TauriPage, label: string, width: number, height: number): Promise<void> {
  const labelJson = JSON.stringify(label)
  await page.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:window|set_size', {
    label: ${labelJson},
    value: { Logical: { width: ${String(Math.round(width))}, height: ${String(Math.round(height))} } }
  })`)
}

/** How many grow-then-remeasure rounds before we accept what we have. */
const FIT_ROUNDS = 4
/** Slack added to each grow so a reflow that gains a line doesn't need another round. */
const FIT_SLACK_PX = 12

/**
 * The UI zoom ladder the fit falls back to, in order, once the DISPLAY is the
 * limit and the content still doesn't fit. `appearance.textSize` allows 75–150 in
 * steps of 5 (`definitions/appearance.ts`), so 75 is the real floor; there's no
 * point stepping finer than this when the alternative is a clipped screenshot.
 * ❗ Every surface captured below 100 is recorded in the report, so a translator
 * knows the text in that image is smaller than what a user sees.
 */
const ZOOM_LADDER = [90, 80, 75]

/** What fitting a window to a surface achieved, and what it couldn't. */
export interface FitOutcome {
  /** Logical px the window grew by; 0 when the content already fit. */
  grewBy: number
  /** The UI zoom the shot was taken at. Below `DEFAULT_UI_ZOOM` means the display ran out. */
  zoom: number
  /** Content still not fully visible after growing (and any zoom step), in logical px. */
  residual: number
  /** Boxes clipping content that nothing can scroll into view: a UI bug, not a capture artifact. */
  unreachable: string[]
}

/** A completed fit plus the undo that puts the window (and zoom) back. */
export interface WindowFit {
  outcome: FitOutcome
  restore: () => Promise<void>
}

/**
 * Grows `windowLabel` until `selector`'s content fits, and hands back an undo.
 *
 * Order matters: the window grows FIRST (a translator judges text at its real
 * size), and the UI zoom only comes down once the display itself is the limit.
 * Every step is measured off live layout, so nothing here depends on knowing a
 * dialog's height. Returns null when `selector` doesn't match anything, so a
 * caller can pass a speculative selector.
 *
 * Best-effort by design: a window that refuses to resize leaves a `residual` in
 * the outcome rather than failing the surface, because a slightly-cut screenshot
 * still beats no screenshot.
 */
export async function fitWindowToContent(
  page: TauriPage,
  windowLabel: string,
  selector: FrameSelector,
  settle: (page: TauriPage) => Promise<void>,
  setZoom: (page: TauriPage, percent: number) => Promise<void>,
): Promise<WindowFit | null> {
  const first = await measureFit(page, selector)
  if (first === null) return null

  const originalWidth = first.innerWidth
  const originalHeight = first.innerHeight
  let zoom = DEFAULT_UI_ZOOM
  let latest = first

  const growRounds = async (): Promise<void> => {
    for (let round = 0; round < FIT_ROUNDS; round++) {
      const m = await measureFit(page, selector)
      if (m === null) return
      latest = m
      if (m.need <= 1) return
      const target = Math.min(m.innerHeight + m.need + FIT_SLACK_PX, m.maxHeight)
      if (target <= m.innerHeight + 1) return // the display is the limit
      await setWindowSize(page, windowLabel, m.innerWidth, target)
      await settle(page)
    }
    const done = await measureFit(page, selector)
    if (done !== null) latest = done
  }

  await growRounds()

  // Still clipped with the window as tall as the display allows: shrink the UI
  // until it fits. This is the ONLY case where zooming out is the right answer.
  for (const step of ZOOM_LADDER) {
    if (latest.need <= 1) break
    await setZoom(page, step)
    zoom = step
    await settle(page)
    await growRounds()
  }

  // ❗ Zooming out only earns its keep if it actually DELIVERS the whole surface.
  // A settings section that is simply long (the shortcut list runs thousands of
  // px past the display) is still cut off at 75 %, so keeping the reduced zoom
  // would hand a translator smaller text AND a clipped image: strictly worse than
  // the honest 100 % shot. So a ladder that never achieved fit is rolled all the
  // way back, and the surface is photographed at real size with its residual.
  if (latest.need > 1 && zoom !== DEFAULT_UI_ZOOM) {
    console.log(
      `[i18n-capture] fit: ${String(zoom)}% still left ${String(latest.need)}px unshown, so this surface goes ` +
        'back to 100% — a smaller-AND-clipped image helps nobody',
    )
    await setZoom(page, DEFAULT_UI_ZOOM)
    zoom = DEFAULT_UI_ZOOM
    await settle(page)
    await growRounds()
  }

  const outcome: FitOutcome = {
    grewBy: Math.max(0, latest.innerHeight - originalHeight),
    zoom,
    residual: Math.max(0, latest.need),
    unreachable: latest.unreachable,
  }

  const restore = async (): Promise<void> => {
    if (zoom !== DEFAULT_UI_ZOOM) {
      // ❗ Not swallowed: the zoom is a cross-window app SETTING, so a failed
      // restore leaves every later surface silently captured at this reduced
      // zoom, with only this one surface's report entry admitting it.
      await setZoom(page, DEFAULT_UI_ZOOM).catch((err: unknown) => {
        console.warn(
          `[i18n-capture] could not restore the UI zoom to ${String(DEFAULT_UI_ZOOM)}%; later surfaces may be ` +
            `captured at ${String(zoom)}%: ${err instanceof Error ? err.message : String(err)}`,
        )
      })
    }
    await setWindowSize(page, windowLabel, originalWidth, originalHeight).catch(() => {})
    await settle(page)
  }

  return { outcome, restore }
}

/**
 * One element flagged by the clip-overflow scan: a text-bearing node whose
 * content is cut off by its own box (its scroll size exceeds its client size
 * while `overflow` clips). Best-effort heuristic, not proof of a visible defect.
 */
export interface ClipFinding {
  /** A short CSS-ish path to the element (tag + id/classes), for the report. */
  selector: string
  /** The clipped text content (trimmed, capped), so the reviewer can spot it. */
  text: string
  /** Horizontal overflow in px (`scrollWidth - clientWidth`), 0 if none. */
  overflowX: number
  /** Vertical overflow in px (`scrollHeight - clientHeight`), 0 if none. */
  overflowY: number
}

/** surface label → the clip findings detected on it (empty array = clean). */
export const clipFindings: Record<string, ClipFinding[]> = {}

/**
 * Scans the page's DOM for text that its own box clips, and records the findings
 * under `label`. The heuristic: a text-bearing element whose `scrollWidth >
 * clientWidth` (or `scrollHeight > clientHeight`) by more than a small tolerance,
 * AND whose computed `overflow` in that axis hides/clips the spill (so the text
 * is actually cut off, not scrollable into view). We skip naturally-scrollable
 * containers (`auto`/`scroll`), visually-hidden accessibility nodes (`sr-only` /
 * the announcer, which always clip by design), and elements with no direct text.
 * This finds the common pseudolocale failures: a truncated button/label/header
 * where +40% text no longer fits. It is a HEURISTIC: it can miss a clip that an
 * ancestor masks, and can flag a deliberately-ellipsized label (which may be
 * acceptable design). Treat the report as a list of spots to eyeball, not a hard
 * pass/fail. No-op outside an overflow pass.
 */
export async function scanForClipping(page: TauriPage, label: string): Promise<void> {
  if (!isOverflowPass) return
  try {
    const findings = await page.evaluate<ClipFinding[]>(`(function() {
      var TOL = 1; // sub-pixel rounding tolerance
      var out = [];
      var nodes = document.querySelectorAll('body *');
      for (var i = 0; i < nodes.length; i++) {
        var el = nodes[i];
        // Only text-bearing elements: at least one direct, non-whitespace text node.
        var hasText = false;
        for (var c = 0; c < el.childNodes.length; c++) {
          var n = el.childNodes[c];
          if (n.nodeType === 3 && n.textContent && n.textContent.trim() !== '') { hasText = true; break; }
        }
        if (!hasText) continue;
        var s = getComputedStyle(el);
        if (s.display === 'none' || s.visibility === 'hidden' || parseFloat(s.opacity) === 0) continue;
        // Skip visually-hidden accessibility nodes: the standard 'sr-only' /
        // screen-reader-announcer pattern collapses the box to a 1px clip-rect, so
        // it ALWAYS "clips" its text by design and is never seen by a user. Flagging
        // it is pure noise that buries real overflow. Detect it by the conventional
        // class names AND by the tell-tale tiny clip box (clientW/H <= 1px).
        var cls = (typeof el.className === 'string') ? el.className : '';
        if (/\\bsr-only\\b/.test(cls) || el.id === 'svelte-announcer') continue;
        if (el.clientWidth <= 1 || el.clientHeight <= 1) continue;
        var ofx = el.scrollWidth - el.clientWidth;
        var ofy = el.scrollHeight - el.clientHeight;
        // Only count an axis whose overflow is hidden/clipped/ellipsed (text is
        // actually cut off). 'auto'/'scroll' means the user can reach it, 'visible'
        // means it spills (a layout-break, caught separately below).
        var clipsX = (s.overflowX === 'hidden' || s.overflowX === 'clip' || s.textOverflow === 'ellipsis');
        var clipsY = (s.overflowY === 'hidden' || s.overflowY === 'clip');
        var hitX = ofx > TOL && clipsX;
        var hitY = ofy > TOL && clipsY;
        if (!hitX && !hitY) continue;
        // Build a short selector for the report.
        var sel = el.tagName.toLowerCase();
        if (el.id) sel += '#' + el.id;
        if (cls) {
          var selCls = cls.trim().split(/\\s+/).slice(0, 3).join('.');
          if (selCls) sel += '.' + selCls;
        }
        var txt = (el.textContent || '').trim().replace(/\\s+/g, ' ');
        if (txt.length > 80) txt = txt.slice(0, 80) + '…';
        out.push({ selector: sel, text: txt, overflowX: hitX ? ofx : 0, overflowY: hitY ? ofy : 0 });
      }
      // De-dup identical (selector,text) rows an ancestor + child can both produce.
      var seen = {};
      var dedup = [];
      for (var k = 0; k < out.length; k++) {
        var key = out[k].selector + '|' + out[k].text;
        if (seen[key]) continue;
        seen[key] = true;
        dedup.push(out[k]);
      }
      return dedup;
    })()`)
    clipFindings[label] = findings
    if (findings.length > 0) {
      console.warn(`[i18n-overflow] ${label}: ${String(findings.length)} clipped element(s)`)
    }
  } catch {
    // Best-effort: a window that closed mid-scan, or whose eval didn't come back,
    // just gets no findings rather than failing the run.
    clipFindings[label] ??= []
  }
}

/** How long a stray toast gets to leave the DOM after its close button is clicked, and how often to look. */
const STRAY_DISMISS_TIMEOUT_MS = 3000
const STRAY_DISMISS_POLL_MS = 25

/** How much of a toast's text identifies it, in the before/after comparison. */
const TOAST_TEXT_KEY_LENGTH = 80

/**
 * The live toast layer as a list of short text keys, one per `.toast`. Text is
 * the identity that matters: two toasts with the same words are interchangeable
 * for "did something new appear", and a toast carries no stable DOM id to key on.
 */
export async function readToastSignature(page: TauriPage): Promise<string[]> {
  return page.evaluate<string[]>(`(function(){
    var out = [];
    var toasts = document.querySelectorAll('.toast');
    for (var i = 0; i < toasts.length; i++) {
      out.push((toasts[i].textContent || '').trim().replace(/\\s+/g, ' ').slice(0, ${String(TOAST_TEXT_KEY_LENGTH)}));
    }
    return out;
  })()`)
}

/**
 * The toasts on screen that `expected` doesn't account for. Multiset semantics:
 * two identical staged toasts allow two on screen, a third is a stray.
 */
export function straysIn(live: string[], expected: string[]): string[] {
  const budget = new Map<string, number>()
  for (const key of expected) budget.set(key, (budget.get(key) ?? 0) + 1)
  const strays: string[] = []
  for (const key of live) {
    const left = budget.get(key) ?? 0
    if (left > 0) budget.set(key, left - 1)
    else strays.push(key)
  }
  return strays
}

/**
 * Dismisses every toast `expected` doesn't account for and waits for the nodes to
 * DETACH, which is exact (`ToastItem` has no exit transition: dismissal removes
 * the node) and needs no duration.
 *
 * Called before each shot so a stray can't be photographed, most often mid-slide-
 * in and translucent over the dialog. `expected` is empty for the vast majority
 * of surfaces; a toast surface passes the signature it staged.
 */
export async function clearStrayToasts(page: TauriPage, expected: string[]): Promise<void> {
  const strays = straysIn(await readToastSignature(page), expected)
  if (strays.length === 0) return
  const expectedJson = JSON.stringify(expected)
  await page.evaluate(`(function(){
    var budget = {};
    var expected = ${expectedJson};
    for (var i = 0; i < expected.length; i++) budget[expected[i]] = (budget[expected[i]] || 0) + 1;
    var toasts = document.querySelectorAll('.toast');
    for (var j = 0; j < toasts.length; j++) {
      var key = (toasts[j].textContent || '').trim().replace(/\\s+/g, ' ').slice(0, ${String(TOAST_TEXT_KEY_LENGTH)});
      if (budget[key] > 0) { budget[key] -= 1; continue; }
      var close = toasts[j].querySelector('.toast-close');
      if (close) close.click();
    }
  })()`)
  // Wait for the nodes to DETACH rather than for a duration: `ToastItem` has no
  // exit transition, so removal from the DOM is the whole dismissal. A hand-rolled
  // loop (not `expect.poll`) because this module stays free of the Playwright test
  // runtime so its pure parts are unit-testable, and because naming the stubborn
  // toast beats a bare "expected 0".
  const deadline = Date.now() + STRAY_DISMISS_TIMEOUT_MS
  for (;;) {
    const left = straysIn(await readToastSignature(page), expected)
    if (left.length === 0) return
    if (Date.now() >= deadline) {
      throw new Error(
        `Toasts this surface never staged would not dismiss within ${String(STRAY_DISMISS_TIMEOUT_MS)} ms: ` +
          left.join('; '),
      )
    }
    await new Promise((resolve) => setTimeout(resolve, STRAY_DISMISS_POLL_MS))
  }
}
