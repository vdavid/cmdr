/**
 * Every dialog's body content lines up with its title, on both edges.
 *
 * `ModalDialog` owns the side inset (`--spacing-dialog`, matching the title bar
 * and the footer), so a body section that re-adds one of its own is a double
 * inset: the title starts at 20px and the text at 44px. That drifted unnoticed
 * across a dozen dialogs, because nothing measured it and the offenders paid the
 * second inset with a DIFFERENT token (`--spacing-xl`), so no grep for the
 * dialog token found them.
 *
 * The measurement is content edges, not boxes: a `<p>` with `padding-left` still
 * has a box that spans the body's full width, so comparing element rects alone
 * would have called the original bug aligned. The check adds each element's own
 * padding to get the edge the WORDS start at, and walks down the first-child
 * chain so a plain wrapper `<div>` doesn't hide the section that pays the inset.
 *
 * The one carve-out is a SURFACE (a side border, a background, or a form
 * control): padding inside one is that surface's own chrome, so it's measured by
 * its box instead, and the descent stops there rather than running into its
 * innards. `TextInput` is why — the `.text-field` WRAPPER carries the border,
 * background, and `--spacing-input`, so both "measure the content edge" and
 * "descend to the `<input>` and measure its box" report a perfectly normal field
 * as 9px misaligned.
 *
 * It drives the dialog gallery (`DIALOG_GALLERY_ENTRIES`), the same registry the
 * i18n capture walks, so a dialog joins this check the day it gets a gallery row.
 * Every dialog is measured in ONE test and reported together: a per-dialog test
 * would stop at the first failure, and the useful output here is the whole list.
 *
 * ❗ **It needs a gallery-carrying binary, which the standard E2E lane does NOT
 * build.** The gallery's gate is `import.meta.env.DEV || __CMDR_I18N_CAPTURE__`
 * and the disk-backed rows' fixture command is `#[cfg(debug_assertions)]`; a plain
 * `pnpm test:e2e:playwright:build` binary is a release build with neither. Against
 * one, this test SKIPS with the build recipe rather than reporting phantom
 * failures. The build that satisfies it is the capture build (`pnpm i18n:capture
 * --build` makes the same one). Giving the normal lane a gallery would mean a
 * build-time E2E define plus widening that Rust cfg — a deliberate call, not a
 * side effect of this spec.
 *
 * ❗ A full-bleed section (a divider or scroll region that deliberately reaches
 * the panel edge, cancelling the inset with a negative inline margin) is a real
 * exception, and it fails this check by design: put its dialog id in
 * `FULL_BLEED_DIALOGS` with a reason rather than loosening the tolerance.
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady, dismissAllToasts, dismissOverlay } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { DIALOG_GALLERY_ENTRIES } from '../../src/lib/dialog-gallery/gallery-registry.js'

/**
 * Dialogs whose first body section reaches the panel edge on purpose. Each needs
 * a reason: this list is the exception log, not a mute button.
 */
const FULL_BLEED_DIALOGS: Record<string, string> = {}

/**
 * Sub-pixel slack only. The inset is one token on every edge, so a real
 * difference is 4px at the very least; anything under a pixel is layout rounding.
 */
const TOLERANCE_PX = 1

interface InsetReading {
  /** Distance from the panel's border box to where the TITLE's words start. */
  titleLeft: number
  /** Same, for the first body section's content. */
  bodyLeft: number
  titleRight: number
  bodyRight: number
  /** Class of the element that paid the inset, for a failure message you can act on. */
  bodyElement: string
}

/** Why a dialog produced no reading, in words the log can print as-is. */
interface InsetUnreadable {
  reason: string
}

type InsetResult = InsetReading | InsetUnreadable

function isReading(result: InsetResult): result is InsetReading {
  return 'bodyLeft' in result
}

/**
 * Reads both content edges of the title and of the body's first section, in the
 * open dialog, or says why it couldn't.
 */
function readInsets(page: TauriPage, dialogId: string): Promise<InsetResult> {
  return page.evaluate<InsetResult>(`(function () {
        var overlay = document.querySelector('[data-dialog-id="${dialogId}"]')
        if (!overlay) return { reason: 'no dialog is open; the state never mounted' }
        var panel = overlay.querySelector('.modal-dialog')
        var title = overlay.querySelector('.dialog-title-bar h2')
        var body = overlay.querySelector('.modal-body')
        if (!panel || !title || !body) return { reason: 'open, but not a ModalDialog (a soft sheet has no .modal-body)' }

        // Padding on a SURFACE (anything drawing a side border or a background: a
        // text field, a tinted card, a well) is that surface's own chrome, and what
        // has to line up with the title is its BOX. Padding on a bare block is an
        // inset, and what has to line up is where its WORDS start. That's the whole
        // rule; without it a text field reads as 9px misaligned (\`--spacing-input\`
        // plus its border) while looking perfectly correct on screen.
        function isSurface(el) {
            if (/^(input|textarea|select|button)$/.test(el.tagName.toLowerCase())) return true
            var style = getComputedStyle(el)
            if (parseFloat(style.borderLeftWidth) > 0 || parseFloat(style.borderRightWidth) > 0) return true
            if (style.backgroundImage !== 'none') return true
            // Whitespace-insensitive: don't depend on how the engine spaces the components.
            var bg = String(style.backgroundColor).replace(/\\s/g, '')
            return bg !== '' && bg !== 'transparent' && bg !== 'rgba(0,0,0,0)'
        }

        function horizontalPadding(el) {
            var style = getComputedStyle(el)
            return parseFloat(style.paddingLeft) + parseFloat(style.paddingRight)
        }

        // Start at the first ELEMENT child and descend its first-element-child chain
        // while each link is a pass-through: an only child, no surface of its own, no
        // padding. A bare wrapper \`<div>\` must not hide the section that pays the
        // inset, and equally must not let the descent run past the section INTO a
        // control's innards.
        var section = body.firstElementChild
        if (!section) return { reason: 'the body is empty' }
        while (
            !isSurface(section) &&
            horizontalPadding(section) === 0 &&
            section.children.length === 1 &&
            section.firstElementChild
        ) {
            section = section.firstElementChild
        }

        function edges(el) {
            var rect = el.getBoundingClientRect()
            if (isSurface(el)) return { left: rect.left, right: rect.right }
            var style = getComputedStyle(el)
            return {
                left: rect.left + parseFloat(style.paddingLeft),
                right: rect.right - parseFloat(style.paddingRight),
            }
        }

        var panelRect = panel.getBoundingClientRect()
        var titleEdges = edges(title)
        var sectionEdges = edges(section)
        return {
            titleLeft: titleEdges.left - panelRect.left,
            bodyLeft: sectionEdges.left - panelRect.left,
            titleRight: panelRect.right - titleEdges.right,
            bodyRight: panelRect.right - sectionEdges.right,
            bodyElement: section.tagName.toLowerCase() + (section.className ? '.' + String(section.className).split(' ').join('.') : ''),
        }
    })()`)
}

/** The landmark set the dev-only fixture-directory command returns. */
interface FixtureDirPayload {
  root: string
  destinationDir: string
  existingFolderName: string
  existingFileName: string
  nestedPath: string
}

/**
 * The disk-backed rows' throwaway directory, or `null` when this binary has no
 * `create_dialog_gallery_fixtures` (it's `#[cfg(debug_assertions)]`, which a plain
 * release build leaves off). Those five rows then go unmeasured and say so, which
 * beats taking the whole sweep down with them.
 */
async function createFixtureDir(page: TauriPage): Promise<FixtureDirPayload | null> {
  return page
    .evaluate<FixtureDirPayload>(`window.__TAURI_INTERNALS__.invoke('create_dialog_gallery_fixtures', {})`)
    .catch(() => null)
}

/** Opens one gallery state exactly the way Debug > Soft dialogs does. */
function openGalleryState(
  page: TauriPage,
  dialogId: string,
  stateId: string,
  fixtures: FixtureDirPayload | null,
): Promise<void> {
  const payload = JSON.stringify({ dialogId, stateId, fixtures })
  return page.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
        event: 'debug-open-gallery-dialog',
        payload: ${payload}
    })`)
}

/**
 * Does this binary carry the gallery? Opens the one row that needs nothing staged
 * (the short alert) and looks for it. A direct capability test: probing the Rust
 * fixture command instead would answer a different question (that gate is
 * `debug_assertions`, the gallery's is a Vite define).
 */
async function probeGallery(page: TauriPage): Promise<boolean> {
  await openGalleryState(page, 'alert', 'short', null)
  const live = await page
    .waitForSelector('[data-dialog-id="alert"]', 4000)
    .then(() => true)
    .catch(() => false)
  if (live) await dismissOverlay(page).catch(() => {})
  return live
}

test.describe('Dialog body inset', () => {
  test.describe.configure({ timeout: 180000 })

  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
  })

  test('every dialog’s first body section lines up with its title', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage

    // The gallery is baked in only where its gate is on (`import.meta.env.DEV ||
    // __CMDR_I18N_CAPTURE__`), so a plain E2E binary has no listener to answer the
    // trigger and every dialog would read as "didn't open". Skip loudly with the
    // build recipe rather than reporting 30 phantom failures.
    const galleryLive = await probeGallery(page)
    test.skip(
      !galleryLive,
      'This binary carries no dialog gallery. Build one that does:\n' +
        '  CMDR_I18N_CAPTURE_BUILD=1 node scripts/tauri-wrapper.ts build --no-bundle --target $(rustc -vV | grep host | cut -d" " -f2) ' +
        '-- --features playwright-e2e,virtual-mtp --config profile.release.debug-assertions=true',
    )

    const fixtures = await createFixtureDir(page)
    if (fixtures === null) {
      console.log('[dialog-inset] no fixture directory (release build): the five disk-backed rows go unmeasured.')
    }

    const misaligned: string[] = []
    const unread: string[] = []
    let measured = 0

    for (const entry of DIALOG_GALLERY_ENTRIES) {
      // Settings- and viewer-hosted rows render over the main window here, but the
      // panel geometry is the dialog's own, so they're worth measuring all the same.
      if (entry.status !== 'ready') continue
      const state = entry.states[0]
      if (!state) continue

      await openGalleryState(page, entry.dialogId, state.id, fixtures)
      const opened = await page
        .waitForSelector(`[data-dialog-id="${entry.dialogId}"]`, 8000)
        .then(() => true)
        .catch(() => false)

      const result: InsetResult = opened
        ? await readInsets(page, entry.dialogId)
        : { reason: 'the trigger produced no dialog within 8s' }
      if (!isReading(result)) {
        unread.push(`${entry.dialogId}/${state.id}: ${result.reason}`)
      } else {
        const reading = result
        measured++
        const leftGap = Math.abs(reading.titleLeft - reading.bodyLeft)
        const rightGap = Math.abs(reading.titleRight - reading.bodyRight)
        const exempt = entry.dialogId in FULL_BLEED_DIALOGS
        if (!exempt && (leftGap > TOLERANCE_PX || rightGap > TOLERANCE_PX)) {
          misaligned.push(
            `${entry.dialogId}/${state.id} (${reading.bodyElement}): ` +
              `title ${reading.titleLeft.toFixed(1)}/${reading.titleRight.toFixed(1)}px, ` +
              `body ${reading.bodyLeft.toFixed(1)}/${reading.bodyRight.toFixed(1)}px (left/right)`,
          )
        }
      }

      await dismissOverlay(page).catch(() => {})
      await expect
        .poll(async () => !(await page.isVisible(`[data-dialog-id="${entry.dialogId}"]`).catch(() => false)), {
          timeout: 3000,
        })
        .toBeTruthy()
        .catch(() => {})
    }

    // The virtual MTP device announces itself on its own schedule, so its connect
    // toast can land mid-sweep; anything still on screen at the end trips the
    // afterEach leak guard, however little this spec had to do with it.
    await dismissAllToasts(page).catch(() => {})

    // A run that measured almost nothing would pass silently, which is the one
    // outcome this check must never produce.
    console.log(`[dialog-inset] measured ${String(measured)} dialogs, ${String(unread.length)} unreadable`)
    for (const line of unread) console.log(`[dialog-inset] unread: ${line}`)
    expect(measured, 'no dialog could be measured; the gallery trigger is probably broken').toBeGreaterThan(10)
    expect(misaligned, `dialogs whose body content doesn't line up with the title:\n${misaligned.join('\n')}`).toEqual(
      [],
    )
  })
})
