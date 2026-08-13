/**
 * A resizable dialog's edges and corners are grabbable, and dragging one moves
 * only the edge under the pointer.
 *
 * Both halves need a REAL engine, which is why they're here and not in
 * `ModalDialog.svelte.test.ts` (which covers the arithmetic in jsdom):
 *
 * - The bands hang over the panel's edge on purpose, so an `overflow: hidden`
 *   creeping back onto `.modal-dialog` would clip them away. `elementFromPoint` is
 *   the honest test for that: it's the engine's own hit test, so it also catches a
 *   band that renders but sits under something else.
 * - The panel is CENTERED, so widening it slides its layout box by half the growth.
 *   Only a real layout run proves the drag offset pays that back and the edge the
 *   user isn't holding stays put.
 *
 * The operation log is the both-axes case (`resizable`) and Go to path the
 * width-only one (`resizable="horizontal"`); both open through their production
 * menu command, so no gallery-carrying build is needed.
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady, dismissOverlay, dismissAllToasts, dispatchMenuCommand } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/** Sub-pixel slack only: a real drift here is tens of pixels. */
const TOLERANCE_PX = 1.5

const OPERATION_LOG = 'operation-log'
const GO_TO_PATH = 'go-to-path'

/**
 * What each dialog renders only once its body has settled, which is what every
 * measurement below needs.
 *
 * The operation log is the one that can't gate on the panel: it mounts in a spinner
 * state and its first page arrives over IPC, so the rows land some frames after
 * `.modal-dialog` exists and the panel grows from the spinner's height to the list's
 * (hundreds of pixels, up to the panel's own max-height). Any `before` rect taken in
 * that window belongs to a different layout than the `after` one. Rows, the
 * nothing-yet notice, and the read-failed notice are the three settled ends.
 */
const SETTLED = {
  [OPERATION_LOG]: `[data-dialog-id="${OPERATION_LOG}"] .op-list, [data-dialog-id="${OPERATION_LOG}"] .notice`,
  [GO_TO_PATH]: `[data-dialog-id="${GO_TO_PATH}"] .dialog-body`,
}

interface PanelRect {
  left: number
  right: number
  top: number
  bottom: number
  width: number
  height: number
}

async function openDialog(page: TauriPage, dialogId: keyof typeof SETTLED, command: string): Promise<void> {
  await dispatchMenuCommand(page, command)
  await page.waitForSelector(`[data-dialog-id="${dialogId}"] .modal-dialog`, 5000)
  await page.waitForSelector(SETTLED[dialogId], 5000)
}

function panelRect(page: TauriPage, dialogId: string): Promise<PanelRect> {
  return page.evaluate<PanelRect>(`(function () {
        var panel = document.querySelector('[data-dialog-id="${dialogId}"] .modal-dialog')
        var r = panel.getBoundingClientRect()
        return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, width: r.width, height: r.height }
    })()`)
}

/**
 * What the engine finds two pixels outside the panel's edge — the pixels a user
 * aims at when reaching for a window edge — reported as `direction:cursor` so one
 * assertion covers both the hit test and the arrow it shows.
 */
function bandOutside(page: TauriPage, dialogId: string, edge: string): Promise<string> {
  return page.evaluate<string>(`(function () {
        var panel = document.querySelector('[data-dialog-id="${dialogId}"] .modal-dialog')
        var r = panel.getBoundingClientRect()
        var points = {
            e: [r.right + 2, (r.top + r.bottom) / 2],
            w: [r.left - 2, (r.top + r.bottom) / 2],
            n: [(r.left + r.right) / 2, r.top - 2],
            s: [(r.left + r.right) / 2, r.bottom + 2],
            se: [r.right + 2, r.bottom + 2],
            nw: [r.left - 2, r.top - 2],
            ne: [r.right + 2, r.top - 2],
            sw: [r.left - 2, r.bottom + 2]
        }
        var point = points['${edge}']
        var hit = document.elementFromPoint(point[0], point[1])
        if (!hit) return 'nothing is there'
        if (!hit.classList.contains('resize-band')) return 'not a band: ' + (hit.className || hit.tagName)
        return hit.getAttribute('data-direction') + ':' + getComputedStyle(hit).cursor
    })()`)
}

/** Presses a band, drags it by (dx, dy), and releases. */
function dragBand(page: TauriPage, dialogId: string, direction: string, dx: number, dy: number): Promise<void> {
  return page.evaluate(`(function () {
        var band = document.querySelector('[data-dialog-id="${dialogId}"] .resize-band[data-direction="${direction}"]')
        var r = band.getBoundingClientRect()
        var x = (r.left + r.right) / 2
        var y = (r.top + r.bottom) / 2
        band.dispatchEvent(new PointerEvent('pointerdown', { clientX: x, clientY: y, bubbles: true }))
        document.dispatchEvent(new PointerEvent('pointermove', { clientX: x + ${String(dx)}, clientY: y + ${String(dy)} }))
        document.dispatchEvent(new PointerEvent('pointerup', {}))
    })()`)
}

function bandDirections(page: TauriPage, dialogId: string): Promise<string[]> {
  return page.evaluate<string[]>(`(function () {
        var bands = document.querySelectorAll('[data-dialog-id="${dialogId}"] .resize-band')
        return [].map.call(bands, function (band) { return band.getAttribute('data-direction') })
    })()`)
}

test.describe('Dialog edge resizing', () => {
  test.describe.configure({ timeout: 30000 })

  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
  })

  test.afterEach(async ({ tauriPage }) => {
    await dismissOverlay(tauriPage).catch(() => {})
    // A virtual MTP device announces itself whenever it connects, and that toast
    // outlives the test that happened to be running.
    await dismissAllToasts(tauriPage).catch(() => {})
  })

  test('offers every edge and corner outside the panel, each with its own cursor', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await openDialog(page, OPERATION_LOG, 'log.operationLog')

    expect(await bandOutside(page, OPERATION_LOG, 'e')).toBe('e:ew-resize')
    expect(await bandOutside(page, OPERATION_LOG, 'w')).toBe('w:ew-resize')
    expect(await bandOutside(page, OPERATION_LOG, 'n')).toBe('n:ns-resize')
    expect(await bandOutside(page, OPERATION_LOG, 's')).toBe('s:ns-resize')
    // Corners come last in the DOM, so they win where they overlap an edge.
    expect(await bandOutside(page, OPERATION_LOG, 'se')).toBe('se:nwse-resize')
    expect(await bandOutside(page, OPERATION_LOG, 'nw')).toBe('nw:nwse-resize')
    expect(await bandOutside(page, OPERATION_LOG, 'ne')).toBe('ne:nesw-resize')
    expect(await bandOutside(page, OPERATION_LOG, 'sw')).toBe('sw:nesw-resize')
  })

  test('drags the east edge and leaves the other three where they were', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await openDialog(page, OPERATION_LOG, 'log.operationLog')
    const before = await panelRect(page, OPERATION_LOG)

    await dragBand(page, OPERATION_LOG, 'e', -60, 0)
    const after = await panelRect(page, OPERATION_LOG)

    expect(Math.abs(after.width - (before.width - 60))).toBeLessThan(TOLERANCE_PX)
    expect(Math.abs(after.left - before.left)).toBeLessThan(TOLERANCE_PX)
    // Nothing asserts the height here: until someone drags it, the panel's height is
    // its content's, and a narrower panel reflows (this dialog grows ~19px).
  })

  test('drags the north edge and leaves the bottom where it was', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await openDialog(page, OPERATION_LOG, 'log.operationLog')
    const before = await panelRect(page, OPERATION_LOG)

    await dragBand(page, OPERATION_LOG, 'n', 0, 40)
    const after = await panelRect(page, OPERATION_LOG)

    expect(Math.abs(after.height - (before.height - 40))).toBeLessThan(TOLERANCE_PX)
    expect(Math.abs(after.bottom - before.bottom)).toBeLessThan(TOLERANCE_PX)
    expect(Math.abs(after.width - before.width)).toBeLessThan(TOLERANCE_PX)
  })

  // Once a vertical drag has pinned the height, a horizontal one must leave it alone:
  // the two axes are carried together, so a width-only drag that forgot the height
  // would silently hand it back to the content.
  test('keeps a dragged height through a later width drag', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await openDialog(page, OPERATION_LOG, 'log.operationLog')

    await dragBand(page, OPERATION_LOG, 'n', 0, 30)
    const pinned = await panelRect(page, OPERATION_LOG)

    await dragBand(page, OPERATION_LOG, 'e', -40, 0)
    const after = await panelRect(page, OPERATION_LOG)

    expect(Math.abs(after.height - pinned.height)).toBeLessThan(TOLERANCE_PX)
    expect(Math.abs(after.width - (pinned.width - 40))).toBeLessThan(TOLERANCE_PX)
  })

  // A width-only dialog exposes no band that could change its height, so there's
  // nothing to drag into a strip of dead space above the footer.
  test('a width-only dialog exposes the side edges and nothing else', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await openDialog(page, GO_TO_PATH, 'nav.goToPath')

    expect(await bandDirections(page, GO_TO_PATH)).toEqual(['w', 'e'])
    // Above the panel there's only the scrim, so the pointer keeps its normal arrow.
    expect(await bandOutside(page, GO_TO_PATH, 'n')).toContain('not a band')
  })
})
