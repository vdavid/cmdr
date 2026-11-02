/**
 * No word in any dialog breaks mid-word, and nothing in one runs past its panel,
 * at 125 % zoom.
 *
 * `ModalDialog`'s body once wrapped with `overflow-wrap: anywhere`, which gave
 * every flex item inside it a one-character minimum: a squeezed row broke "You"
 * into "Yo / u" at 125 % zoom (#700). The body now uses `break-word`, which
 * keeps every word whole unless it ALONE outgrows the line. The price is that a
 * long unbreakable string (a path, a file name) in a row that isn't set up to
 * shrink (`min-width: 0` plus an ellipsis or the middle truncation) widens the
 * row past the panel edge. So this checks both halves, on every state of every
 * dialog in the gallery (`DIALOG_GALLERY_ENTRIES`), long-path states included:
 *
 * - **A split word**: a run of letters and digits whose first and last
 *   characters sit on different lines. A word wider than the body's whole line
 *   is exempt, since breaking it is the only alternative to overflowing.
 *   Hyphens, slashes, and spaces are break opportunities, so a path that wraps
 *   at its slashes splits no word. Scripts that break between characters by
 *   design (CJK, Thai) are skipped.
 * - **Horizontal overflow**: an element or a line of text past the panel's
 *   left or right edge, or a scroll container inside the panel that scrolls
 *   sideways. Content inside a clipping ancestor (an ellipsis cell, a scroller)
 *   is judged by that ancestor.
 *
 * 125 % is the zoom #700 surfaced at, and the step most people use who use one.
 * The zoom rides the production `appearance.textSize` setting, set before and
 * reset after each test, so no later spec on the shard inherits it.
 *
 * The states run in fixed-size batches, one test each: a single sweep would blow
 * the per-test duration budget, and one test per dialog would leave
 * `transfer-error` (50 states) far over it. Every finding in a batch is reported
 * together.
 *
 * ❗ Needs a gallery-carrying binary, like `dialog-inset.spec.ts`
 * (`skipWithoutGallery`: a local skip, a CI failure).
 */

import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import { ensureAppReady, dismissAllToasts, getFixtureRoot, runAnimationFramesOnTimers } from './helpers.js'
import {
  closeGallerySurface,
  createFixtureDir,
  openGalleryState,
  skipWithoutGallery,
} from './dialog-gallery-helpers.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { DIALOG_GALLERY_ENTRIES } from '../../src/lib/dialog-gallery/gallery-registry.js'

const ZOOM_PERCENT = 125

/** States per test, sized to keep each near the 2 s per-test budget. */
const BATCH_SIZE = 12

interface GalleryState {
  dialogId: string
  stateId: string
  /** The gallery harness renders it, so the next preview replaces it (no `openedBy`). */
  harnessRendered: boolean
}

/**
 * Every state the gallery can open from a fixture alone. Real-operation rows are
 * out for the reason `dialog-inset.spec.ts` gives: an Escape-based close can't
 * cleanly end the real copy they start.
 */
const STATES: GalleryState[] = DIALOG_GALLERY_ENTRIES.filter(
  (entry) => entry.status === 'ready' && entry.openedBy !== 'real-operation',
).flatMap((entry) =>
  entry.states.map((state) => ({
    dialogId: entry.dialogId,
    stateId: state.id,
    harnessRendered: entry.openedBy === undefined,
  })),
)

const BATCHES: GalleryState[][] = []
for (let i = 0; i < STATES.length; i += BATCH_SIZE) BATCHES.push(STATES.slice(i, i + BATCH_SIZE))

interface WrapReading {
  /** Each mid-word break, as `"word" in <element>`. */
  splits: string[]
  /** Each horizontal overflow, as `<element>: what and by how much`. */
  overflows: string[]
}

type WrapResult = WrapReading | { reason: string }

function isReading(result: WrapResult): result is WrapReading {
  return 'splits' in result
}

/** Reads the open dialog's split words and horizontal overflows, or says why it couldn't. */
function readWrapping(page: TauriPage, dialogId: string): Promise<WrapResult> {
  return page.evaluate<WrapResult>(`(function () {
        var overlay = document.querySelector('[data-dialog-id="${dialogId}"]')
        if (!overlay) return { reason: 'no dialog is open; the state never mounted' }
        var panel = overlay.querySelector('.modal-dialog') || overlay
        var panelRect = panel.getBoundingClientRect()
        var body = panel.querySelector('.modal-body')

        // The widest line a word could ever get: the body's content box (the panel's
        // own for a surface that isn't a ModalDialog). A word wider than this has to
        // break or overflow, so breaking it is right.
        var lineBox = body || panel
        var lineStyle = getComputedStyle(lineBox)
        var lineWidth = lineBox.clientWidth - parseFloat(lineStyle.paddingLeft) - parseFloat(lineStyle.paddingRight)

        function describe(el) {
            var cls = typeof el.className === 'string' && el.className.trim() ? '.' + el.className.trim().split(/\\s+/).join('.') : ''
            return el.tagName.toLowerCase() + cls
        }

        // The nearest ancestor (or the element itself, with includeSelf) that clips
        // horizontally, below the panel's own layers. The panel's clipping
        // \`.modal-content\` (resizable and fill-body dialogs) doesn't count: it IS
        // the panel edge.
        function clipOf(el, includeSelf) {
            for (var a = includeSelf ? el : el.parentElement; a && a !== panel; a = a.parentElement) {
                if (a.classList.contains('modal-content')) return null
                var style = getComputedStyle(a)
                if (style.position === 'fixed') return a
                if (style.overflowX !== 'visible') return a
            }
            return null
        }

        function skipped(el) {
            return !!el.closest('.resize-band, input, textarea, select, [aria-hidden="true"]')
        }

        var splits = []
        var overflows = []
        var range = document.createRange()

        function rectOf(node, start, end) {
            range.setStart(node, start)
            range.setEnd(node, end)
            return range.getClientRects()
        }

        // The box a single character is drawn in. A character right after a soft
        // break also reports a zero-width rect at the END of the previous line (the
        // break's caret position), so the first rect alone would call every word
        // after a wrapped hyphen split.
        function glyphOf(node, offset) {
            var rects = rectOf(node, offset, offset + 1)
            for (var r = 0; r < rects.length; r++) if (rects[r].width > 0) return rects[r]
            return null
        }

        // The content width of the block that lays out \`el\`'s lines.
        function lineOf(el) {
            for (var a = el; a && a !== panel; a = a.parentElement) {
                var style = getComputedStyle(a)
                if (style.display.indexOf('inline') === 0 || style.display === 'contents') continue
                return a.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight)
            }
            return lineWidth
        }

        // Separators no natural-language word carries, or a monospace run (code, a hash).
        function isNonWord(token, el) {
            if (/[\\/\\\\._:@~=#-]/.test(token) || /\\p{L}\\p{N}|\\p{N}\\p{L}/u.test(token)) return true
            return /mono/i.test(getComputedStyle(el).fontFamily) || !!el.closest('code, pre, kbd, samp')
        }

        // ── Split words ──
        var word = /[\\p{L}\\p{M}\\p{N}]{2,}/gu
        var breaksBetweenCharacters = /[\\p{Script=Han}\\p{Script=Hiragana}\\p{Script=Katakana}\\p{Script=Hangul}\\p{Script=Thai}]/u
        var walker = document.createTreeWalker(panel, NodeFilter.SHOW_TEXT)
        var node
        while ((node = walker.nextNode())) {
            var parent = node.parentElement
            if (!parent || skipped(parent)) continue
            var text = node.textContent
            if (!text.trim()) continue
            // Visually hidden text (\`sr-only\`) wraps one letter per line inside its 1px box.
            var clip = clipOf(parent, true)
            if (clip && clip.clientWidth <= 2) continue
            if (getComputedStyle(parent).visibility === 'hidden') continue
            word.lastIndex = 0
            var match
            while ((match = word.exec(text))) {
                if (breaksBetweenCharacters.test(match[0])) continue
                var start = match.index
                var end = start + match[0].length
                var first = glyphOf(node, start)
                var last = glyphOf(node, end - 1)
                if (!first || !last || first.height === 0) continue
                if (Math.abs(last.top - first.top) < first.height / 2) continue
                // The whole whitespace-delimited token: a path that outgrows the line
                // has to break somewhere, and breaking it is the fix, not the bug.
                var tokenStart = start
                while (tokenStart > 0 && !/\\s/.test(text[tokenStart - 1])) tokenStart--
                var tokenEnd = end
                while (tokenEnd < text.length && !/\\s/.test(text[tokenEnd])) tokenEnd++
                var token = text.slice(tokenStart, tokenEnd)
                var tokenWidth = 0
                var tokenRects = rectOf(node, tokenStart, tokenEnd)
                for (var i = 0; i < tokenRects.length; i++) tokenWidth += tokenRects[i].width
                if (tokenWidth > lineWidth - 1) continue
                // A path, a file name, or code isn't a word: breaking one that alone
                // outgrows its own line (a half-width card, a table cell) is
                // \`break-word\` doing its job.
                var ownLine = lineOf(parent)
                if (isNonWord(token, parent) && tokenWidth > ownLine - 1) continue
                splits.push(
                    '"' + match[0] + '" in ' + describe(parent) + ' (token "' + token.slice(0, 60) + '" ' +
                        tokenWidth.toFixed(0) + 'px, its line ' + ownLine.toFixed(0) + 'px, body line ' + lineWidth.toFixed(0) + 'px)',
                )
            }
        }

        // ── Horizontal overflow ──
        function past(rect) {
            if (rect.width === 0) return 0
            return Math.max(rect.right - panelRect.right, panelRect.left - rect.left)
        }

        var elements = panel.querySelectorAll('*')
        for (var e = 0; e < elements.length; e++) {
            var el = elements[e]
            if (skipped(el)) continue
            var style = getComputedStyle(el)
            if (style.display === 'none') continue
            // A scroller that scrolls sideways is content wider than the panel, however
            // politely it's contained.
            if ((style.overflowX === 'auto' || style.overflowX === 'scroll') && el.scrollWidth > el.clientWidth + 1 && !clipOf(el, false)) {
                overflows.push(describe(el) + ': scrolls sideways by ' + (el.scrollWidth - el.clientWidth) + 'px')
            }
            if (clipOf(el, false)) continue
            var by = past(el.getBoundingClientRect())
            if (by > 1) overflows.push(describe(el) + ': ' + by.toFixed(1) + 'px past the panel edge')
        }

        // Text spills out of a box that stays inside, so its lines are checked too.
        walker = document.createTreeWalker(panel, NodeFilter.SHOW_TEXT)
        while ((node = walker.nextNode())) {
            var host = node.parentElement
            if (!host || skipped(host) || !node.textContent.trim() || clipOf(host, true)) continue
            var lines = rectOf(node, 0, node.textContent.length)
            for (var l = 0; l < lines.length; l++) {
                var spill = past(lines[l])
                if (spill > 1) {
                    overflows.push(describe(host) + ': text "' + node.textContent.trim().slice(0, 40) + '" ' + spill.toFixed(1) + 'px past the panel edge')
                    break
                }
            }
        }

        return { splits: splits, overflows: overflows }
    })()`)
}

/** Sets the UI zoom through the production setting, and waits for the scale to land. */
async function setZoom(page: TauriPage, percent: number): Promise<void> {
  await page.evaluate(`window.__cmdrI18nCapture.setTextSize(${JSON.stringify(String(percent))})`)
  await expect
    .poll(
      async () =>
        page.evaluate<number>(
          `parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--font-scale')) || 1`,
        ),
      { timeout: waitBudget(3000) },
    )
    .toBeCloseTo(percent / 100, 2)
}

/**
 * Closes a state, and reports whether it went. A harness-rendered preview is swapped
 * for the short alert, which then closes on Escape: an in-flight state ("Delete in
 * flight") refuses Escape and has dead buttons by design, and the harness shows one
 * preview at a time. One the app mounts itself goes through `closeGallerySurface`.
 */
async function closeState(page: TauriPage, state: GalleryState): Promise<boolean> {
  if (!state.harnessRendered) return closeGallerySurface(page, state.dialogId)
  await openGalleryState(page, 'alert', 'short', null)
  await page.waitForSelector('[data-dialog-id="alert"]', waitBudget(4000)).catch(() => {})
  const replaced = !(await page.isVisible(`[data-dialog-id="${state.dialogId}"]`).catch(() => false))
  return (state.dialogId === 'alert' || replaced) && (await closeGallerySurface(page, 'alert'))
}

/** Two frames, so a dialog that measures itself on mount has laid out at the new size. */
function settleLayout(page: TauriPage): Promise<void> {
  return page.evaluate(
    `new Promise(function (resolve) { requestAnimationFrame(function () { requestAnimationFrame(function () { resolve() }) }) })`,
  )
}

test.describe('Dialog wrapping at 125 % zoom', () => {
  test.describe.configure({ timeout: waitBudget(60000) })

  test.beforeEach(async ({ tauriPage }) => {
    // The fixture tree is shared, and the conflict specs replace its contents
    // wholesale; `dialog-inset.spec.ts` explains the 10 s this saves.
    recreateFixtures(getFixtureRoot())
    await ensureAppReady(tauriPage)
    // `settleLayout` waits on frames, which an occluded window never fires: it hung the
    // eval for its whole 30 s, and the dialog it left up failed the rest of the shard.
    await runAnimationFramesOnTimers(tauriPage)
  })

  test.afterEach(async ({ tauriPage }) => {
    await setZoom(tauriPage as TauriPage, 100).catch(() => {})
    // The virtual MTP device's connect toast can land mid-batch.
    await dismissAllToasts(tauriPage).catch(() => {})
  })

  BATCHES.forEach((batch, index) => {
    const first = batch[0]
    const last = batch[batch.length - 1]
    const span = `${first.dialogId}/${first.stateId} … ${last.dialogId}/${last.stateId}`

    test(`batch ${String(index + 1)} keeps every word whole and inside the panel (${span})`, async ({ tauriPage }) => {
      const page = tauriPage as TauriPage
      await skipWithoutGallery(page)
      await setZoom(page, ZOOM_PERCENT)
      const fixtures = await createFixtureDir(page)

      const findings: string[] = []
      const unread: string[] = []
      const stuck: string[] = []

      for (const { dialogId, stateId, harnessRendered } of batch) {
        await openGalleryState(page, dialogId, stateId, fixtures)
        const opened = await page
          .waitForSelector(`[data-dialog-id="${dialogId}"]`, waitBudget(8000))
          .then(() => true)
          .catch(() => false)
        if (opened) await settleLayout(page)

        const result: WrapResult = opened
          ? await readWrapping(page, dialogId)
          : { reason: 'the trigger produced no dialog within 8s' }
        if (!isReading(result)) {
          unread.push(`${dialogId}/${stateId}: ${result.reason}`)
        } else {
          for (const split of result.splits) findings.push(`${dialogId}/${stateId}: split word ${split}`)
          for (const overflow of result.overflows) findings.push(`${dialogId}/${stateId}: overflow ${overflow}`)
        }

        if (!(await closeState(page, { dialogId, stateId, harnessRendered }))) stuck.push(`${dialogId}/${stateId}`)
      }

      for (const line of unread) console.log(`[dialog-wrapping] unread: ${line}`)
      expect(unread.length, 'most of this batch never opened; the gallery trigger is probably broken').toBeLessThan(
        batch.length,
      )
      expect(stuck, `gallery surfaces still open after the batch:\n${stuck.join('\n')}`).toEqual([])
      expect(findings, `words split mid-word or content past the panel edge:\n${findings.join('\n')}`).toEqual([])
    })
  })
})
