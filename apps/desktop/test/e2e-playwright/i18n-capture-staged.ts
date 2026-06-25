/**
 * Mock-staged special-surface captures for the i18n screenshot-capture driver
 * (`i18n-capture.spec.ts`).
 *
 * These are the mock-staged surfaces: the ones that
 * need a feature-compiled binary (`virtual-mtp`), a `#[cfg(debug_assertions)]`
 * mock the release-with-debug-assertions capture build now honors
 * (`CMDR_MOCK_LICENSE`, `CMDR_MOCK_FDA`), or a backend event the frontend can
 * stage from the capture sink.
 *
 * Split into two families by launch shape:
 *  - MAIN-pass surfaces (`captureMtpSurfaces`, `captureDownloadToasts`,
 *    `captureQuickLookHint`): reachable in the default capture launch (the
 *    virtual MTP device auto-registers under E2E mode; the download + quick-look
 *    toasts fire from a frontend-emitted event / keypress). The spec calls these
 *    in the main pass.
 *  - PER-LAUNCH surfaces (`captureLicensePass`, `captureFdaOnboardingPass`):
 *    each needs a launch-time env (`CMDR_MOCK_LICENSE` / `CMDR_MOCK_FDA`) read
 *    once at startup, so they run in their OWN orchestrator launch and MERGE
 *    into the report the main pass wrote (see `scripts/i18n-capture.ts`'s
 *    multi-launch loop and the spec's pass dispatch).
 *
 * All use the shared engines in `i18n-capture-helpers.ts`.
 */

import { expect } from './fixtures.js'
import { ensureAppReady, dismissOverlay, dispatchMenuCommand, getFixtureRoot } from './helpers.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { initMcpClient, mcpSelectVolume, mcpAwaitItem } from '../e2e-shared/mcp-client.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { type SurfaceEntry, captureCall, captureSurface, captureToastSurface } from './i18n-capture-helpers.js'

/** The virtual MTP device's internal-storage volume name (matches `mtp.spec.ts`). */
const MTP_INTERNAL_STORAGE = 'Virtual Pixel 9 - Internal Storage'

/**
 * Captures the MTP surfaces, reachable in the MAIN capture pass because the
 * `virtual-mtp` capture build auto-registers the fake device under E2E mode (see
 * `src-tauri/src/mtp/virtual_device.rs` `decide_startup_root`).
 *
 * - `mtp-browse`: select the virtual device's Internal Storage on the focused
 *   pane and capture the browse view (the volume breadcrumb + the device's file
 *   list), recording any `mtp.*` / file-list keys unique to an MTP volume. Plain
 *   mounted markup, so the normal `captureSurface` rerender path records its keys.
 * - `mtp-connected-toast`: the sticky connect toast (`mtp.connectedToast.*`). It
 *   fires from the `mtp-device-connected` Tauri event; the layout's listener adds
 *   the toast (gated by `fileOperations.mtpConnectionWarning`, default true). The
 *   real device already auto-connected at startup BEFORE the sink was enabled, so
 *   we RE-EMIT the typed event with the sink active (snapshot-before-trigger) to
 *   resolve + record the keys. The toast dedupes by id (`mtp-connected`), so any
 *   startup instance is dismissed first.
 */
export async function captureMtpSurfaces(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  await ensureAppReady(main)
  await initMcpClient(main)

  // ── MTP browse view ──────────────────────────────────────────────────────
  await captureSurface('mtp-browse', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'mtp-browse')
    await captureCall<boolean>(main, 'enable')
    // Select the virtual device's Internal Storage on the left (focused) pane and
    // wait for a known fixture entry so the listing has actually swapped.
    await mcpSelectVolume('left', MTP_INTERNAL_STORAGE)
    await mcpAwaitItem('left', 'Documents')
    return { page: main }
  })
  await captureCall(main, 'disable').catch(() => {})

  // ── MTP connected toast (snapshot-before-trigger) ─────────────────────────
  // Re-emit the typed `mtp-device-connected` event with the sink enabled so the
  // toast's snapshot-resolved copy records. The toast body only reads the device
  // NAME (title) + a static mac/other body, so a minimal payload suffices; the
  // empty `storages` array is fine (the toast doesn't iterate it).
  await captureToastSurface('mtp-connected-toast', report, failed, main, async () => {
    // Dismiss any startup connect toast first so the dedupe-by-id (`mtp-connected`)
    // doesn't no-op our re-emit, then fire the event the layout listener handles.
    await main.evaluate(`(function(){
      var toasts = document.querySelectorAll('.toast .toast-close');
      for (var i = 0; i < toasts.length; i++) toasts[i].click();
    })()`)
    await main.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'mtp-device-connected',
      payload: { deviceId: 'mtp-virtual', deviceName: 'Virtual Pixel 9', storages: [] }
    })`)
  })
}

/**
 * Captures the download teaching toast (`downloads.toast.*`), reachable in the
 * MAIN pass: the toast fires from the backend `download-detected` Tauri event,
 * which the frontend `event-bridge` fans out to `DownloadToastContent`. We emit
 * the typed event directly with the sink active (snapshot-before-trigger).
 *
 * Two gates the default (hermetic) store satisfies:
 *  - `behavior.fileSystemWatching.downloadsNotifications` defaults to `in-app`
 *    (so the toast path runs).
 *  - the in-app `downloads.goToLatest` shortcut (`⌘J`) is bound by default, so a
 *    teachable hint exists and the bridge doesn't skip the toast.
 *
 * The bridge also re-checks `downloads_watcher_status().fdaPending` and bails if
 * the FDA gate is pending. The capture launch sets `CMDR_MOCK_FDA=granted`
 * (debug-assertions build), so the gate reads open and the toast surfaces.
 */
export async function captureDownloadToasts(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  await ensureAppReady(main)

  await captureToastSurface('toast-download', report, failed, main, async () => {
    // A plausible Downloads-folder file. `inSubdir:false` keeps it a direct child
    // so the subdir line stays off this surface (its key couples elsewhere).
    await main.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'download-detected',
      payload: {
        path: '/tmp/Downloads/report.pdf',
        parentDir: '/tmp/Downloads',
        fileName: 'report.pdf',
        observedAtMs: ${String(Date.now())},
        inSubdir: false,
        sizeBytes: 1048576
      }
    })`)
  })
}

/**
 * Captures the Quick Look educational hint toast (`fileExplorer.quickLookHint.*`),
 * now reachable in the MAIN pass: the capture binary launches with a hermetic
 * default `CMDR_DATA_DIR`, so `fileExplorer.suppressQuickLookHint` reads its
 * default (`false`) and a plain Space in the file list fires the hint (it was
 * suppressed in the developer's real store before the data-dir fix).
 *
 * The toast is a mounted component with `<Trans>` copy, but it's added at
 * keypress time, so it's snapshot-staged like the other toasts: enable the sink,
 * press Space on a focused file entry, catch the toast, record.
 */
export async function captureQuickLookHint(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  recreateFixtures(getFixtureRoot())
  await ensureAppReady(main)

  await captureToastSurface('quick-look-hint', report, failed, main, async () => {
    // Move the cursor onto a real file (not the synthetic `..`) and press plain
    // Space, which toggles selection AND fires `maybeShowQuickLookHint()`.
    await main.evaluate(`(function(){
      var pane = document.querySelector('.file-pane.is-focused') || document.querySelector('.file-pane');
      var entry = pane && pane.querySelector('.file-entry[data-filename="file-a.txt"]');
      if (entry) {
        entry.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
        entry.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      }
    })()`)
    await main.evaluate(`(function(){
      var pane = document.querySelector('.file-pane.is-focused') || document.querySelector('.file-pane');
      var target = pane || document.body;
      target.dispatchEvent(new KeyboardEvent('keydown', { key: ' ', code: 'Space', bubbles: true }));
    })()`)
  })
}

/**
 * License-pass surfaces, captured in a SEPARATE launch per `CMDR_MOCK_LICENSE`
 * value (the mock is read once at startup under `#[cfg(debug_assertions)]`, which
 * the capture build turns on for the release profile). Each launch's `pass`
 * names which mock is active so this captures only that state's surface and
 * MERGES into the existing report.
 *
 * - `commercial` / `perpetual` → the About dialog renders the paid copy
 *   (`licensing.about.commercial*` / `.perpetual`). Opened via `app.about`.
 * - `personal_reminder` → the commercial-reminder modal auto-opens at boot
 *   (`+page.svelte` shows it when `status.showCommercialReminder`).
 * - `expired` → the expiration modal auto-opens at boot
 *   (`status.type === 'expired' && status.showModal`).
 *
 * The LICENSE DETAILS view (the LicenseKeyDialog with a committed key) is NOT
 * here: it reads `getLicenseInfo()` (the stored, signature-verified key), which
 * the env mock doesn't populate: it needs a real committed test key, out of
 * scope. It stays document-skipped in the spec.
 */
export async function captureLicensePass(
  pass: string,
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  if (pass === 'license:commercial' || pass === 'license:perpetual') {
    // The paid About is opened on demand by command, so the app can be brought to
    // a clean ready state first.
    await ensureAppReady(main)
    const label = pass === 'license:perpetual' ? 'about-perpetual' : 'about-commercial'
    await captureSurface(label, report, failed, async () => {
      await captureCall(main, 'setSurface', label)
      await captureCall<boolean>(main, 'enable')
      await dispatchMenuCommand(main, 'app.about')
      await main.waitForSelector('[data-dialog-id="about"]', 5000)
      return { page: main }
    })
    await dismissOverlay(main).catch(() => {})
    await captureCall(main, 'disable').catch(() => {})
    return
  }

  // The reminder + expiration modals AUTO-OPEN at boot (driven by the mock
  // `AppStatus`). Crucially, DON'T call `ensureAppReady` here: it dispatches an
  // Escape to clear "lingering" modals, which would dismiss the very boot modal
  // we want. Instead wait for the file list (app loaded), then for the modal's
  // `data-dialog-id`. Enable the sink + rerender to record the mount-time keys.
  const bootModal = async (label: string, dialogId: string): Promise<void> => {
    await captureSurface(label, report, failed, async () => {
      // App-loaded signal without touching modals (no Escape, no route reset that
      // would unmount the boot dialog).
      await main.waitForSelector('.file-entry', 15000)
      await main.waitForSelector(`[data-dialog-id="${dialogId}"]`, 10000)
      await captureCall(main, 'reset')
      await captureCall(main, 'setSurface', label)
      await captureCall<boolean>(main, 'enable')
      return { page: main }
    })
    await dismissOverlay(main).catch(() => {})
    await captureCall(main, 'disable').catch(() => {})
  }

  if (pass === 'license:reminder') {
    await bootModal('commercial-reminder', 'commercial-reminder')
    return
  }
  if (pass === 'license:expired') {
    await bootModal('expiration', 'expiration')
    return
  }
}

/** Onboarding step-1 (FDA) selector + the active-step dot reader, shared with the wizard walk. */
const WIZARD = '[data-dialog-id="onboarding"]'

/**
 * FDA-variant onboarding pass: captures the step-1 (Full Disk Access) banner in
 * a specific `CMDR_MOCK_FDA` state, in its OWN launch (the mock is read by the
 * runtime FDA probe; the wizard's step-1 banner branches on it).
 *
 * The default macOS capture launch already grants FDA, so the wizard's
 * `onboarding-fda` surface shows the already-granted variant. This pass drives
 * the OTHER branches (`notgranted` / `denied`) so the first-ask / revoked copy
 * (`onboarding.fda.*` variants) gets a screenshot. Labeled per pass so each
 * variant couples its own keys.
 */
export async function captureFdaOnboardingPass(
  pass: string,
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  await ensureAppReady(main)
  const label = `onboarding-fda-${pass.replace('fda:', '')}`

  await captureSurface(label, report, failed, async () => {
    await dispatchMenuCommand(main, 'cmdr.openOnboarding')
    await main.waitForSelector(WIZARD, 5000)
    await main.waitForSelector(`${WIZARD} .step-shell`, 5000)
    await captureCall(main, 'reset')
    await captureCall<boolean>(main, 'enable')
    await captureCall(main, 'setSurface', label)
    await captureCall(main, 'rerender')
    return { page: main }
  })

  // Close the wizard so it can't leak into a later launch's state (best-effort).
  // The loop self-guards on visibility, so it's a no-op if the wizard never opened.
  for (let i = 0; i < 6; i++) {
    if (!(await main.isVisible(WIZARD).catch(() => false))) break
    await main
      .evaluate(`(function(){
          var btns = document.querySelectorAll('${WIZARD} .primary-slot button');
          if (btns.length > 0) btns[btns.length - 1].click();
        })()`)
      .catch(() => {})
    await expect
      .poll(async () => !(await main.isVisible(WIZARD).catch(() => false)), { timeout: 1500 })
      .toBeTruthy()
      .catch(() => {})
  }
}
