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
 * ❗ **It needs a gallery-carrying binary.** Every E2E build makes one:
 * `CMDR_E2E_BUILD=1` (set by `test:e2e:playwright:build` and the Linux Docker
 * build) turns on the `__CMDR_DIALOG_GALLERY__` define, and the disk-backed rows'
 * fixture command compiles under `feature = "playwright-e2e"` as well as
 * `debug_assertions`. A hand-rolled build that sets neither leaves this test
 * nothing to measure: locally it SKIPS with the recipe, but under CI it FAILS,
 * because a silent skip there would read as coverage that isn't happening.
 *
 * ❗ A full-bleed section (a divider or scroll region that deliberately reaches
 * the panel edge, cancelling the inset with a negative inline margin) is a real
 * exception, and it fails this check by design: put its dialog id in
 * `FULL_BLEED_DIALOGS` with a reason rather than loosening the tolerance.
 */

import { test, expect } from './fixtures.js'
import {
  ensureAppReady,
  dismissAllToasts,
  dismissOverlay,
  getFixtureRoot,
  acceptOnboardingTermsIfPresent,
} from './helpers.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
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

/**
 * Closes whatever the gallery just opened, and reports whether it actually went.
 *
 * Escape closes a `ModalDialog`, but a soft sheet can swallow it — the onboarding wizard
 * does, deliberately — and a surface left up swallows the keystrokes of every spec after
 * it on the shard. That's not hypothetical: it killed four `file-operations` rename tests
 * three specs downstream, each reporting a missing `.rename-input`. So the fallback is the
 * wizard's own contract (`onboarding.spec.ts`): click the footer's forward button until it
 * unmounts. The caller fails the sweep on anything still open rather than carrying it.
 */
async function closeGallerySurface(page: TauriPage, dialogId: string): Promise<boolean> {
  const selector = `[data-dialog-id="${dialogId}"]`
  const isOpen = async (): Promise<boolean> => page.isVisible(selector).catch(() => false)

  await dismissOverlay(page).catch(() => {})
  // Four steps plus slack, matching the wizard's own bound.
  for (let i = 0; i < 6 && (await isOpen()); i++) {
    // The onboarding wizard's Beta step blocks its forward button until the terms are ticked.
    await acceptOnboardingTermsIfPresent(page)
    await page.evaluate(`(function() {
        var btns = document.querySelectorAll('${selector} .primary-slot button');
        if (btns.length > 0) btns[btns.length - 1].click();
    })()`)
    await expect
      .poll(isOpen, { timeout: 1000 })
      .toBeFalsy()
      .catch(() => {})
  }
  return !(await isOpen())
}

test.describe('Dialog body inset', () => {
  test.describe.configure({ timeout: 180000 })

  test.beforeEach(async ({ tauriPage }) => {
    // The fixture tree is shared, and the conflict specs that run just before this one
    // replace its contents wholesale. Without the recreate, `ensureAppReady` waits out
    // its 10 s on a left pane still showing THEIR files and the sweep never starts.
    recreateFixtures(getFixtureRoot())
    await ensureAppReady(tauriPage)
  })

  test('every dialog’s first body section lines up with its title', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage

    // No gallery in this binary means no listener to answer the trigger, and every
    // dialog would read as "didn't open". Locally that's a build to redo, so skip
    // with the recipe; in CI it's the whole check quietly not running, so fail.
    const galleryLive = await probeGallery(page)
    const noGallery =
      'This binary carries no dialog gallery (`CMDR_E2E_BUILD=1` sets the ' +
      '`__CMDR_DIALOG_GALLERY__` define). Rebuild with `pnpm test:e2e:playwright:build`.'
    if (!galleryLive && process.env.CI) throw new Error(noGallery)
    test.skip(!galleryLive, noGallery)

    const fixtures = await createFixtureDir(page)
    if (fixtures === null) {
      console.log('[dialog-inset] no fixture directory (release build): the five disk-backed rows go unmeasured.')
    }

    const misaligned: string[] = []
    const unread: string[] = []
    const stuck: string[] = []
    let measured = 0

    for (const entry of DIALOG_GALLERY_ENTRIES) {
      // Settings- and viewer-hosted rows render over the main window here, but the
      // panel geometry is the dialog's own, so they're worth measuring all the same.
      if (entry.status !== 'ready') continue
      // `.at(0)`, not `[0]`: a registry row may carry NO states (`states: []`),
      // which indexing can't express without `noUncheckedIndexedAccess` — so the
      // guard below reads as unreachable to the type checker and gets linted
      // away, taking a real crash guard with it.
      const state = entry.states.at(0)
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

      if (!(await closeGallerySurface(page, entry.dialogId))) {
        stuck.push(`${entry.dialogId}/${state.id}`)
      }
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
    expect(
      stuck,
      `gallery surfaces still open when the sweep ended. One left up swallows every later spec's keystrokes on this shard:\n${stuck.join('\n')}`,
    ).toEqual([])
    expect(misaligned, `dialogs whose body content doesn't line up with the title:\n${misaligned.join('\n')}`).toEqual(
      [],
    )
  })
})
