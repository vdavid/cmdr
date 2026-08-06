/**
 * Registry-driven soft-dialog captures for the i18n screenshot-capture driver
 * (`i18n-capture.spec.ts`).
 *
 * Most soft dialogs are hard to evoke on purpose: the bulk-rename review needs a
 * wired agent, the operation log needs a populated history, the transfer error
 * needs a failed write of a specific kind. Hand-staging each one here would be
 * dozens of bespoke blocks, and dialog 34 would be missed the same way its
 * predecessors were. So this pass drives the DIALOG GALLERY instead:
 * `DIALOG_GALLERY_ENTRIES` already enumerates every registered soft dialog with
 * its reviewable states, and the main window already listens for
 * `debug-open-gallery-dialog` to open any `(dialogId, stateId)` pair with real
 * fixtures. Iterating the registry means a dialog gets a screenshot the day it
 * gets a gallery row, with no change here.
 *
 * `+layout.svelte` and `listener-setup.ts` gate the gallery on
 * `import.meta.env.DEV || __CMDR_I18N_CAPTURE__`. This capture build sets the
 * second flag; a production build sets neither. See
 * `src/lib/dialog-gallery/DETAILS.md`.
 *
 * Two deliberate limits, both about not lying to translators:
 *
 *  - **Main-window hosts only.** The gallery renders every row over the main
 *    window, including the three dialogs that really live in the settings or
 *    viewer window. A screenshot of a settings dialog floating over the file
 *    panes would show a backdrop it never has in production, so those rows are
 *    SKIPPED here rather than captured with a caveat nobody reads.
 *  - **Novel states only.** A state is photographed only if it resolves at least
 *    one catalog key no earlier surface in this run recorded. That keeps the run
 *    from writing 92 near-identical PNGs for states whose copy the real-trigger
 *    surfaces already captured, and it's why this pass runs LAST: every faithful
 *    capture gets first claim on its keys.
 *
 * Both drops are recorded in `capture-skipped.json`, which is a tracked file, so
 * a state that stops opening shows up in the diff instead of vanishing quietly.
 * They're prefixed `gallery-redundant:` (the pass working as designed) and
 * `gallery-unavailable:` (a gap someone may want to close), because a diff that
 * can't tell those apart tells you nothing.
 */

import { expect } from './fixtures.js'
import { ensureAppReady, dismissOverlay } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import {
  type SurfaceEntry,
  BlankShotError,
  captureCall,
  keysFor,
  shoot,
  scanForClipping,
  stressLayoutIfWorstCase,
  isOverflowPass,
  overflowLocale,
} from './i18n-capture-helpers.js'
import { DIALOG_GALLERY_ENTRIES } from '../../src/lib/dialog-gallery/gallery-registry.js'

/** The landmark set `create_dialog_gallery_fixtures` returns (camelCase over IPC). */
interface FixtureDirPayload {
  root: string
  destinationDir: string
  existingFolderName: string
  existingFileName: string
  nestedPath: string
}

/**
 * Creates (idempotently) the throwaway directory the disk-backed dialogs work
 * against and returns its landmarks, mirroring what the Debug panel does before
 * it emits a trigger. The dialogs that need it (`delete-confirmation`,
 * `transfer-confirmation`, the two name dialogs, `go-to-path`) scan it for real,
 * so the numbers they display are the ones on disk.
 *
 * The command is `#[cfg(debug_assertions)]`, which the capture build turns on for
 * the release profile, so it's present in this binary and absent from a shipped
 * one.
 */
async function createFixtureDir(main: TauriPage): Promise<FixtureDirPayload> {
  return main.evaluate<FixtureDirPayload>(`window.__TAURI_INTERNALS__.invoke('create_dialog_gallery_fixtures', {})`)
}

/**
 * Asks the main window to open one gallery state, the same way the Debug panel
 * does: emit `debug-open-gallery-dialog` and let the main window's own listener
 * resolve fixtures, seed stores, or dispatch the app command, depending on how
 * that dialog is built (`openedBy` in the registry).
 */
async function openGalleryState(
  main: TauriPage,
  dialogId: string,
  stateId: string,
  fixtures: FixtureDirPayload | null,
): Promise<void> {
  const payload = JSON.stringify({ dialogId, stateId, fixtures })
  await main.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
    event: 'debug-open-gallery-dialog',
    payload: ${payload}
  })`)
}

/**
 * Closes whatever preview is up and waits for it to go, so the next state mounts
 * into a clean window. Best-effort on the Escape itself: a preview that already
 * closed itself must not stop the loop.
 *
 * A preview the app renders from its OWN mount site (the `store-seeded` and
 * `event-seeded` rows) isn't unmounted by the next trigger the way a
 * harness-rendered one is, so one that refuses to close would sit behind the next
 * dialog's shot. That's worth a warning, not a failure: the next surface's keys
 * are still its own, and the log names the culprit.
 */
async function closePreview(main: TauriPage, dialogId: string): Promise<void> {
  const selector = `[data-dialog-id="${dialogId}"]`
  // The onboarding wizard swallows Escape on purpose (a half-finished first run
  // shouldn't be dismissible), so it needs its own exit: click the last button in
  // the primary slot, which is Next on every step and Finish on the last. Leaving
  // it up would be worse than a stray image: `rerender` re-resolves every MOUNTED
  // string, so the wizard's keys would record against the NEXT dialog's surface
  // and couple onboarding copy to a screenshot of the operation log.
  if (dialogId === 'onboarding') {
    for (let i = 0; i < 8; i++) {
      if (!(await main.isVisible(selector).catch(() => false))) break
      await main
        .evaluate(`(function(){
          var btns = document.querySelectorAll('${selector} .primary-slot button');
          if (btns.length > 0) btns[btns.length - 1].click();
        })()`)
        .catch(() => {})
      await expect
        .poll(async () => !(await main.isVisible(selector).catch(() => false)), { timeout: 1500 })
        .toBeTruthy()
        .catch(() => {})
    }
  }
  await dismissOverlay(main).catch(() => {})
  const closed = await expect
    .poll(async () => !(await main.isVisible(selector).catch(() => false)), { timeout: 3000 })
    .toBeTruthy()
    .then(() => true)
    .catch(() => false)
  if (!closed) {
    console.warn(`[i18n-capture] gallery: ${dialogId} stayed open after Escape; it may show behind the next shot.`)
  }
}

/**
 * What one attempted gallery state did, so the caller can tally without
 * re-deriving it. `unavailable` is a documented gap (the preview needs conditions
 * this environment lacks); `failed` is a state that DID open but couldn't be
 * photographed, which must reach `capture-failed.json` and fail the run rather
 * than blend into the skip list.
 */
type StateOutcome = 'captured' | 'redundant' | 'unavailable' | 'failed'

/**
 * Stages, measures, and (if it earns it) photographs ONE gallery state.
 *
 * Opens the preview with the sink already labelled and live, so mount-time `t()`
 * calls are recorded, then re-resolves the mounted strings and reads the keys
 * back. A state whose every key an earlier surface already owns is `redundant`:
 * it gets no PNG and no report entry, which is what keeps this pass from writing
 * ~90 near-identical images. `alreadyCovered` grows with each capture, so two
 * states of the same dialog can't both claim the same shared chrome.
 */
async function captureGalleryState(
  main: TauriPage,
  dialogId: string,
  stateId: string,
  fixtures: FixtureDirPayload | null,
  report: Record<string, SurfaceEntry>,
  alreadyCovered: Set<string>,
): Promise<StateOutcome> {
  const label = `${dialogId}-${stateId}`
  try {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', label)
    await captureCall<boolean>(main, 'enable')
    await openGalleryState(main, dialogId, stateId, fixtures)
    await main.waitForSelector(`[data-dialog-id="${dialogId}"]`, 8000)
    if (isOverflowPass) await captureCall(main, 'setLocale', overflowLocale)
    await captureCall(main, 'setSurface', label)
    await captureCall(main, 'rerender')

    const keys = await keysFor(main, label)
    const novel = keys.filter((k) => !alreadyCovered.has(k))
    if (novel.length === 0) return 'redundant'

    await stressLayoutIfWorstCase(main, 'main')
    const screenshot = `${label}.png`
    await shoot(main, 'main', screenshot)
    report[label] = { screenshot, keys }
    for (const key of keys) alreadyCovered.add(key)
    await scanForClipping(main, label)
    console.log(`[i18n-capture] ${label}: ${String(keys.length)} keys (${String(novel.length)} new) → ${screenshot}`)
    return 'captured'
  } catch (err) {
    // A preview that won't open is a documented gap, not a broken surface: some
    // states need conditions this environment doesn't have (a mounted external
    // drive for the stale-index explainer, say). It lands in the TRACKED skip
    // list, so a state that stops opening shows up in the diff.
    //
    // A shot this pass COULD NOT PHOTOGRAPH is a different animal: the dialog
    // opened, its keys were read, and only the image is bad. Skipping that would
    // hide a blank PNG behind a "documented gap" label, so it fails instead.
    const detail = err instanceof Error ? err.message : String(err)
    if (err instanceof BlankShotError) {
      console.warn(`[i18n-capture] gallery ${label} FAILED: ${detail}`)
      return 'failed'
    }
    console.warn(`[i18n-capture] gallery ${label} SKIPPED: ${detail}`)
    return 'unavailable'
  } finally {
    await closePreview(main, dialogId)
    await captureCall(main, 'disable').catch(() => {})
  }
}

/**
 * Decides whether a registry row can be captured here at all, returning the
 * reason it can't. Keeps the "why we skipped it" rules in one readable place
 * rather than scattered through the loop.
 */
function unshootableReason(entry: (typeof DIALOG_GALLERY_ENTRIES)[number], hasFixtures: boolean): string | null {
  if (entry.status === 'not-triggerable') return 'the registry has no reviewable state for it'
  // A settings/viewer dialog rendered over the file panes shows a backdrop it
  // never has in production; don't photograph it as if it were real.
  if (entry.hostWindow !== 'main') return `it really lives in the ${entry.hostWindow} window`
  if (entry.usesFixtureDir === true && !hasFixtures) return 'its fixture directory is unavailable'
  return null
}

/**
 * Captures every reviewable state of every main-window soft dialog the gallery
 * registry lists, keeping the ones whose copy nothing else in this run recorded.
 *
 * Runs LAST in the main pass, after every real-trigger surface, so a hand-staged
 * capture of the production path always wins its keys over a gallery preview of
 * the same dialog.
 */
export async function captureGalleryDialogs(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
  skipped: string[],
): Promise<void> {
  await ensureAppReady(main)

  // Every key any earlier surface recorded. A gallery state earns a screenshot
  // only by adding to this.
  const alreadyCovered = new Set<string>()
  for (const entry of Object.values(report)) for (const key of entry.keys) alreadyCovered.add(key)

  // One fixture directory for the whole pass: the command is idempotent, but the
  // first call writes a few dozen files and every later one only stats them.
  let fixtures: FixtureDirPayload | null = null
  if (DIALOG_GALLERY_ENTRIES.some((e) => e.usesFixtureDir === true && e.hostWindow === 'main')) {
    try {
      fixtures = await createFixtureDir(main)
    } catch (err) {
      const detail = err instanceof Error ? err.message : String(err)
      console.warn(`[i18n-capture] gallery: no fixture directory, disk-backed dialogs will be skipped: ${detail}`)
    }
  }

  let captured = 0
  const redundant: string[] = []

  // The skip list is tracked, so it's worth saying WHICH kind of skip each was:
  // `redundant` is the pass working as designed, `unavailable` is a gap someone
  // may want to close. A single `gallery:` prefix would blur the two in the diff.
  for (const entry of DIALOG_GALLERY_ENTRIES) {
    const blocked = unshootableReason(entry, fixtures !== null)
    if (blocked !== null) {
      skipped.push(`gallery-unavailable:${entry.dialogId}`)
      console.log(`[i18n-capture] gallery ${entry.dialogId} skipped: ${blocked}.`)
      continue
    }
    const stateFixtures = entry.usesFixtureDir === true ? fixtures : null

    for (const state of entry.states) {
      const label = `${entry.dialogId}-${state.id}`
      // A hand-staged surface already owns this name; never overwrite its PNG.
      if (label in report) {
        skipped.push(`gallery-redundant:${label}`)
        continue
      }
      const outcome = await captureGalleryState(main, entry.dialogId, state.id, stateFixtures, report, alreadyCovered)
      if (outcome === 'captured') captured += 1
      else if (outcome === 'redundant') {
        redundant.push(label)
        skipped.push(`gallery-redundant:${label}`)
      } else if (outcome === 'failed') {
        failed.push(label)
      } else {
        skipped.push(`gallery-unavailable:${label}`)
      }
    }
  }

  // Say what was dropped and why: a silent cap reads as "we covered everything".
  const dropped = redundant.length > 0 ? `: ${redundant.join(', ')}` : ''
  console.log(
    `[i18n-capture] gallery: ${String(captured)} states captured, ${String(redundant.length)} dropped as ` +
      `redundant (every key already on an earlier surface)${dropped}`,
  )
}
