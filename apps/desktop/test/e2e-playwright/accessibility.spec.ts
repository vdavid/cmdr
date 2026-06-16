/**
 * Accessibility audit for Cmdr views and dialogs using axe-core.
 *
 * Injects axe-core into the real Tauri webview via tauriPage.evaluate(),
 * runs a WCAG audit on each view/dialog, and fails on any violation.
 *
 * Each test runs in both dark and light mode. The theme is switched via
 * the Tauri setTheme API between describe blocks.
 *
 * Dialog tests scope the audit to the dialog element itself, avoiding noise
 * from the page behind the overlay.
 */

import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'
import { test, expect } from './fixtures.js'
import {
  closeScopedWindow,
  dismissOverlay,
  dispatchMenuCommand,
  ensureAppReady,
  executeViaCommandPalette,
  moveCursorToFile,
  openSettingsWindowViaProd,
  openViewerWindow,
  pollUntil,
  sleep,
  CTRL_OR_META,
  TRANSFER_DIALOG,
} from './helpers.js'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

const __dirname = path.dirname(fileURLToPath(import.meta.url))

/** Minimal type for the axe-core result shape we care about. */
interface AxeViolation {
  id: string
  impact: 'minor' | 'moderate' | 'serious' | 'critical'
  description: string
  helpUrl: string
  nodes: { html: string; failureSummary: string }[]
}
interface AxeResults {
  violations: AxeViolation[]
}

// Use fixture file from the shared E2E fixture tree. Throw instead of using a
// fallback path: a silent fallback hides setup bugs (the test would try to read
// a non-existent file and fail with a confusing "ENOENT" instead of the actual
// "env var is missing" root cause).
const fixtureRoot = (() => {
  const root = process.env.CMDR_E2E_START_PATH
  if (!root)
    throw new Error('CMDR_E2E_START_PATH env var is not set; fixtures must be created before running this spec')
  return root
})()
const testFilePath = path.join(fixtureRoot, 'left', 'file-a.txt')

/** Read and cache the axe-core source so we only read it from disk once. */
const axeSource = fs.readFileSync(path.resolve(__dirname, '../../node_modules/axe-core/axe.min.js'), 'utf-8')

/** Inject axe-core into the webview if not already present. */
async function injectAxe(tauriPage: PageLike): Promise<void> {
  const hasAxe = await tauriPage.evaluate<boolean>('typeof window.axe !== "undefined"')
  if (!hasAxe) {
    await tauriPage.evaluate(`(function() { ${axeSource}\n; return typeof window.axe; })()`)
  }
}

/**
 * Run axe audit on the full page or a specific element, log violations,
 * and return them grouped by severity.
 *
 * @param scope - Optional CSS selector to scope the audit to a specific element.
 */
async function runAxeAudit(
  tauriPage: PageLike,
  viewName: string,
  scope?: string,
): Promise<{
  critical: AxeViolation[]
  serious: AxeViolation[]
  moderate: AxeViolation[]
  minor: AxeViolation[]
  all: AxeViolation[]
}> {
  await injectAxe(tauriPage)

  // axe.run(context, options): context controls WHAT to scan, options controls HOW.
  // Exclude disabled elements from scanning: WCAG exempts inactive UI components from contrast.
  // Note: axe-core's color-contrast rule has built-in disabled detection, but it relies on
  // the element being natively disabled AND having an opacity < 1. When opacity-based disabled
  // styling is applied, axe may still flag the element if it doesn't recognize the pattern.
  // Explicit context exclusion is more reliable.
  const axeContext = scope
    ? JSON.stringify({ include: [[scope]], exclude: [['[disabled]'], ['.btn:disabled']] })
    : JSON.stringify({ exclude: [['[disabled]'], ['.btn:disabled']] })
  // `color-contrast` is disabled: we check contrast at design time via
  // `scripts/check-a11y-contrast` (deterministic, ~0.3s, no engine-dependent
  // `color-mix()` resolution quirks). Axe stays on for structural rules
  // (ARIA, focus order, labels, keyboard nav) where a running browser is
  // genuinely needed. See `docs/design-system.md` § a11y testing strategy.
  //
  // `resultTypes: ['violations']` tells axe to skip building the
  // `passes`/`incomplete`/`inapplicable` arrays (we only read `violations`).
  // On Linux/Xvfb this drops the Copy and License dialog audits from ~5 s to
  // ~2 s without changing what's tested. axe-core docs:
  // https://github.com/dequelabs/axe-core/blob/develop/doc/API.md#options-parameter.
  const axeOptions = JSON.stringify({
    rules: { 'color-contrast': { enabled: false } },
    resultTypes: ['violations'],
  })
  const axeCall = `axe.run(${axeContext}, ${axeOptions})`
  const results = await tauriPage.evaluate<AxeResults>(axeCall)

  const critical = results.violations.filter((v) => v.impact === 'critical')
  const serious = results.violations.filter((v) => v.impact === 'serious')
  const moderate = results.violations.filter((v) => v.impact === 'moderate')
  const minor = results.violations.filter((v) => v.impact === 'minor')

  // Log all violations for visibility
  for (const v of results.violations) {
    console.log(
      `[axe/${v.impact}] [${viewName}] ${v.id}: ${v.description}\n` +
        `  Help: ${v.helpUrl}\n` +
        v.nodes.map((n) => `  - ${n.html}\n    ${n.failureSummary}`).join('\n'),
    )
  }

  if (results.violations.length > 0) {
    const counts = [
      critical.length && `${String(critical.length)} critical`,
      serious.length && `${String(serious.length)} serious`,
      moderate.length && `${String(moderate.length)} moderate`,
      minor.length && `${String(minor.length)} minor`,
    ]
      .filter(Boolean)
      .join(', ')
    console.log(`\n⚠ [${viewName}] ${counts} violation(s) found`)
  } else {
    console.log(`✓ [${viewName}] No accessibility violations`)
  }

  return { critical, serious, moderate, minor, all: results.violations }
}

/** Open the command palette overlay. */
async function openCommandPalette(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', {
        key: 'p', ctrlKey: ${String(CTRL_OR_META === 'Control')}, metaKey: ${String(CTRL_OR_META === 'Meta')}, shiftKey: true, bubbles: true
    }))`)
  // 3 s: the palette overlay renders in <100 ms on the happy path. Previous
  // 15 s budget exceeded the suite's 8 s per-test ceiling.
  await tauriPage.waitForSelector('.palette-overlay', 3000)
}

/** Open the search dialog overlay. */
async function openSearchDialog(tauriPage: PageLike): Promise<void> {
  await dispatchMenuCommand(tauriPage, 'search.open')
  // 3 s: the search overlay renders in <100 ms on the happy path. Previous
  // 15 s budget exceeded the suite's 8 s per-test ceiling.
  await tauriPage.waitForSelector('.search-overlay', 3000)
}

/** Switch the app theme via Tauri's setTheme API.
 *
 * The app's dark mode is gated on `@media (prefers-color-scheme: dark)` (no
 * `[data-theme]` selector). Tauri's `set_app_theme` overrides the window's
 * `NSAppearance` on macOS, which feeds the WebView's media query. The override
 * is per-window and takes effect on the next paint.
 *
 * Historically this helper polled `--color-bg-primary` against literal hex
 * values to confirm the swap before axe ran. That was for the (now-disabled)
 * `color-contrast` axe rule. Without that rule we don't need the CSS variables
 * to be at any specific value when axe runs; the structural a11y rules don't
 * read computed colors. The poll was timing out at 5 s on every light-mode
 * test on macOS dark-default machines (because the WebView's media query lags
 * the appearance override more than 5 s in WKWebView under load), silently
 * mistesting against the wrong theme and burning ~50 s per run.
 *
 * A forced reflow gives the WebView one repaint to apply the appearance
 * change before axe queries the DOM. That's the minimum needed.
 */
async function setTheme(tauriPage: PageLike, mode: 'dark' | 'light'): Promise<void> {
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:app|set_app_theme', { theme: '${mode}' })`)
  await tauriPage.evaluate(`document.documentElement.offsetHeight`)
}

// ── Tests ───────────────────────────────────────────────────────────────────

for (const mode of ['light', 'dark'] as const) {
  test.describe(`${mode} mode`, () => {
    test.beforeEach(async ({ tauriPage }) => {
      await setTheme(tauriPage, mode)
    })

    test(`main explorer view`, async ({ tauriPage }) => {
      await ensureAppReady(tauriPage)

      const { all } = await runAxeAudit(tauriPage, `Main explorer (${mode})`)
      expect(all, `Found ${String(all.length)} violation(s) in main explorer (${mode})`).toHaveLength(0)
    })

    test(`Copy dialog`, async ({ tauriPage }) => {
      await ensureAppReady(tauriPage)
      await moveCursorToFile(tauriPage, 'file-a.txt')

      await dispatchMenuCommand(tauriPage, 'file.copy')
      await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

      const { all } = await runAxeAudit(tauriPage, `Copy dialog (${mode})`, TRANSFER_DIALOG)
      await dismissOverlay(tauriPage)
      expect(all, `Found ${String(all.length)} violation(s) in Copy dialog (${mode})`).toHaveLength(0)
    })

    test(`Delete dialog`, async ({ tauriPage }) => {
      await ensureAppReady(tauriPage)
      await moveCursorToFile(tauriPage, 'file-a.txt')

      await dispatchMenuCommand(tauriPage, 'file.delete')
      const deleteDialog = '[data-dialog-id="delete-confirmation"]'
      await tauriPage.waitForSelector(deleteDialog, 5000)

      const { all } = await runAxeAudit(tauriPage, `Delete dialog (${mode})`, deleteDialog)
      await dismissOverlay(tauriPage)
      expect(all, `Found ${String(all.length)} violation(s) in Delete dialog (${mode})`).toHaveLength(0)
    })

    test(`Move dialog`, async ({ tauriPage }) => {
      await ensureAppReady(tauriPage)
      await moveCursorToFile(tauriPage, 'file-a.txt')

      await dispatchMenuCommand(tauriPage, 'file.move')
      await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

      const { all } = await runAxeAudit(tauriPage, `Move dialog (${mode})`, TRANSFER_DIALOG)
      await dismissOverlay(tauriPage)
      expect(all, `Found ${String(all.length)} violation(s) in Move dialog (${mode})`).toHaveLength(0)
    })

    test(`About dialog`, async ({ tauriPage }) => {
      await ensureAppReady(tauriPage)

      await executeViaCommandPalette(tauriPage, 'About Cmdr')
      await tauriPage.waitForSelector('[data-dialog-id="about"]', 5000)

      const { all } = await runAxeAudit(tauriPage, `About dialog (${mode})`, '[data-dialog-id="about"]')
      await dismissOverlay(tauriPage)
      expect(all, `Found ${String(all.length)} violation(s) in About dialog (${mode})`).toHaveLength(0)
    })

    test(`License dialog`, async ({ tauriPage }) => {
      await ensureAppReady(tauriPage)

      await executeViaCommandPalette(tauriPage, 'license')
      // 3 s: the license dialog opens in <100 ms on the happy path. Previous
      // 15 s budget exceeded the suite's 8 s per-test ceiling.
      await tauriPage.waitForSelector('[data-dialog-id="license"]', 3000)

      const { all } = await runAxeAudit(tauriPage, `License dialog (${mode})`, '[data-dialog-id="license"]')
      await dismissOverlay(tauriPage)
      expect(all, `Found ${String(all.length)} violation(s) in License dialog (${mode})`).toHaveLength(0)
    })

    test(`Command palette`, async ({ tauriPage }) => {
      await ensureAppReady(tauriPage)

      await openCommandPalette(tauriPage)

      const { all } = await runAxeAudit(tauriPage, `Command palette (${mode})`, '.palette-overlay')

      // Dismiss the palette
      await dismissOverlay(tauriPage)

      expect(all, `Found ${String(all.length)} violation(s) in command palette (${mode})`).toHaveLength(0)
    })

    test(`Search dialog`, async ({ tauriPage }) => {
      await ensureAppReady(tauriPage)

      await openSearchDialog(tauriPage)

      const { all } = await runAxeAudit(tauriPage, `Search dialog (${mode})`, '.search-overlay')

      // Dismiss the search dialog
      await dismissOverlay(tauriPage)

      expect(all, `Found ${String(all.length)} violation(s) in search dialog (${mode})`).toHaveLength(0)
    })

    test(`Settings: all sections`, async ({ tauriPage }) => {
      // Loops through ~15 settings sections, running an axe audit per section. Observed ~8 s
      // light-mode, ~4 s dark-mode in practice; 15 s gives headroom for slow runs without
      // letting a real regression hide behind the default 8 s budget. The other a11y tests in
      // this file run in <1 s and use the default 8 s.
      test.setTimeout(15_000)
      await ensureAppReady(tauriPage)

      // Open settings via the production trigger and scope this audit to the
      // dedicated settings window. The settings UI no longer renders into the
      // main window's `/settings` route: it's a separate WebviewWindow.
      const settings = await openSettingsWindowViaProd(tauriPage as TauriPage)
      try {
        // This test legitimately overrides the default 8 s budget (see
        // `test.setTimeout(15_000)` above) because it audits ~15 settings
        // sections sequentially. The waitForSelector budgets here only need
        // to cover the initial settings-window mount, which is <1 s.
        await settings.waitForSelector('.settings-window', 3000)
        await settings.waitForSelector('.settings-sidebar', 3000)

        // All settings sections with their sidebar paths and data-section-id selectors.
        // Mirror the tree declared in `SettingsSidebar.svelte::TOP_LEVEL_ORDER` and the
        // section bindings in `SettingsContent.svelte`. Top-level sections that own
        // subsections (Appearance, Behavior, File systems, Developer) show a summary card
        // grid when clicked at the top level, so the audit targets each subsection directly.
        const sections: { name: string; path: string[]; sectionId: string }[] = [
          {
            name: 'Appearance > Colors and formats',
            path: ['Appearance', 'Colors and formats'],
            sectionId: 'appearance-colors-and-formats',
          },
          {
            name: 'Appearance > Zoom and density',
            path: ['Appearance', 'Zoom and density'],
            sectionId: 'appearance-zoom-and-density',
          },
          {
            name: 'Appearance > File and folder sizes',
            path: ['Appearance', 'File and folder sizes'],
            sectionId: 'appearance-file-and-folder-sizes',
          },
          { name: 'Appearance > Listing', path: ['Appearance', 'Listing'], sectionId: 'appearance-listing' },
          {
            name: 'Behavior > File operations',
            path: ['Behavior', 'File operations'],
            sectionId: 'behavior-file-operations',
          },
          {
            name: 'Behavior > File system watching',
            path: ['Behavior', 'File system watching'],
            sectionId: 'behavior-file-system-watching',
          },
          { name: 'AI', path: ['AI'], sectionId: 'ai' },
          {
            name: 'File systems > SMB/Network shares',
            path: ['File systems', 'SMB/Network shares'],
            sectionId: 'file-systems-smb-network-shares',
          },
          {
            name: 'File systems > MTP',
            path: ['File systems', 'MTP (Android/Kindle/cameras)'],
            sectionId: 'file-systems-mtp-android-kindle-cameras',
          },
          { name: 'File systems > Git', path: ['File systems', 'Git'], sectionId: 'file-systems-git' },
          { name: 'Viewer', path: ['Viewer'], sectionId: 'viewer' },
          { name: 'Keyboard shortcuts', path: ['Keyboard shortcuts'], sectionId: 'keyboard-shortcuts' },
          {
            name: 'Developer > MCP server',
            path: ['Developer', 'MCP server'],
            sectionId: 'developer-mcp-server',
          },
          { name: 'Developer > Logging', path: ['Developer', 'Logging'], sectionId: 'developer-logging' },
          { name: 'Updates & privacy', path: ['Updates & privacy'], sectionId: 'updates' },
          { name: 'License', path: ['License'], sectionId: 'license' },
          { name: 'Advanced', path: ['Advanced'], sectionId: 'advanced' },
        ]

        const allViolations: { section: string; violations: AxeViolation[] }[] = []

        for (const section of sections) {
          // Click sidebar to navigate to the section
          await settings.evaluate(`(function() {
              var items = document.querySelectorAll('.section-item');
              for (var i = 0; i < items.length; i++) {
                  if (items[i].textContent.trim() === ${JSON.stringify(section.path[section.path.length - 1])}) {
                      items[i].click();
                      break;
                  }
              }
          })()`)

          // Wait for the section to be visible
          const sectionSelector = `[data-section-id="${section.sectionId}"]`
          const sectionVisible = await pollUntil(settings, async () => settings.isVisible(sectionSelector), 5000)
          if (!sectionVisible) {
            console.log(`⚠ Settings section "${section.name}" not visible, skipping`)
            continue
          }

          // Brief settle for sections with async data (for example, File system watching
          // loads dbFileSize which controls the "Clear index" button's disabled state).
          // The pollUntil above already gated on section visibility: this just lets
          // any reactive child updates land before axe inspects the DOM. No specific
          // selector to poll on: each section loads different async data (index size,
          // licensing, AI config, etc.) with no shared "settled" signal.
          // eslint-disable-next-line cmdr/no-arbitrary-sleep-in-e2e -- async section data settle; no shared "settled" signal across the heterogeneous settings sections (index size, license, AI config, etc.) and axe needs a stable DOM
          await sleep(150)

          const { all } = await runAxeAudit(settings, `Settings: ${section.name} (${mode})`)
          if (all.length > 0) {
            allViolations.push({ section: section.name, violations: all })
          }
        }

        const totalViolations = allViolations.reduce((sum, s) => sum + s.violations.length, 0)
        const failedSections = allViolations.map((s) => `${s.section} (${String(s.violations.length)})`).join(', ')
        expect(totalViolations, `Violations in settings (${mode}): ${failedSections}`).toBe(0)
      } finally {
        await closeScopedWindow(tauriPage as TauriPage, settings, 'settings')
      }
    })

    test(`Onboarding wizard (re-entry)`, async ({ tauriPage }) => {
      // The wizard is mounted via the same re-entry path the menu / palette use.
      // On macOS the E2E fixture grants FDA so re-entry shows step 1 (already-granted variant);
      // on Linux re-entry lands directly on step 2 (the Linux skip-step-1 path).
      // We walk the full 4-step flow (FDA → AI → Open beta → Optional), scanning each
      // reachable step for a11y violations. Per-FDA-branch banner coverage stays in tier-3
      // Vitest (`StepAi.a11y.test.ts`).
      await ensureAppReady(tauriPage)

      const WIZARD_SELECTOR = '[data-dialog-id="onboarding"]'

      /** Reads the active wizard step (1-4) from the `aria-current="step"` dot. */
      const readActiveStep = async (): Promise<number | null> =>
        tauriPage.evaluate<number | null>(`(function() {
          var dots = document.querySelectorAll('${WIZARD_SELECTOR} .step-dot');
          for (var i = 0; i < dots.length; i++) {
            if (dots[i].getAttribute('aria-current') === 'step') return i + 1;
          }
          return null;
        })()`)

      /** Clicks the last (forward) primary footer button and waits for the step to advance to `target`. */
      const advanceTo = async (target: number): Promise<void> => {
        await tauriPage.evaluate(`(function() {
          var btns = document.querySelectorAll('${WIZARD_SELECTOR} .primary-slot button');
          if (btns.length > 0) btns[btns.length - 1].click();
        })()`)
        await expect.poll(readActiveStep, { timeout: 3000 }).toBe(target)
      }

      await dispatchMenuCommand(tauriPage, 'cmdr.openOnboarding')
      await tauriPage.waitForSelector(WIZARD_SELECTOR, 3000)

      // The wizard has four step dots (FDA, AI, Open beta, Optional).
      const dotCount = await tauriPage.evaluate<number>(
        `document.querySelectorAll('${WIZARD_SELECTOR} .step-dot').length`,
      )
      expect(dotCount, `Wizard should render four step dots (${mode})`).toBe(4)

      // Scan the wizard at its opening step (step 1 on macOS, step 2 on Linux).
      const { all: openingViolations } = await runAxeAudit(
        tauriPage,
        `Onboarding wizard opening (${mode})`,
        WIZARD_SELECTOR,
      )
      expect(openingViolations, `Violations on wizard opening step (${mode})`).toHaveLength(0)

      const isMac = process.platform === 'darwin'
      if (isMac) {
        // Advance step 1 (already-granted) → step 2 so we can scan it too.
        await advanceTo(2)
        const { all: step2Violations } = await runAxeAudit(
          tauriPage,
          `Onboarding wizard step 2 (${mode})`,
          WIZARD_SELECTOR,
        )
        expect(step2Violations, `Violations on wizard step 2 (${mode})`).toHaveLength(0)
      }

      // Advance step 2 → step 3 (Open beta) via the "Go to open beta" forward button (primary slot, last).
      await advanceTo(3)
      // The Open beta step renders the anonymous-analytics opt-out toggle.
      const hasAnalyticsToggle = await tauriPage.evaluate<boolean>(
        `!!document.querySelector('${WIZARD_SELECTOR} [aria-labelledby="toggle-analytics-title"]')`,
      )
      expect(hasAnalyticsToggle, `Open beta step should render the analytics opt-out toggle (${mode})`).toBe(true)
      const { all: step3Violations } = await runAxeAudit(
        tauriPage,
        `Onboarding wizard step 3 (${mode})`,
        WIZARD_SELECTOR,
      )
      expect(step3Violations, `Violations on wizard step 3 (${mode})`).toHaveLength(0)

      // Advance step 3 → step 4 (Optional) via the "Next" forward button.
      await advanceTo(4)
      const { all: step4Violations } = await runAxeAudit(
        tauriPage,
        `Onboarding wizard step 4 (${mode})`,
        WIZARD_SELECTOR,
      )
      expect(step4Violations, `Violations on wizard step 4 (${mode})`).toHaveLength(0)

      // Finish so the wizard doesn't leak into the next test (the safety net would otherwise fire).
      await tauriPage.evaluate(`(function() {
        var btns = document.querySelectorAll('${WIZARD_SELECTOR} .primary-slot button');
        if (btns.length > 0) btns[btns.length - 1].click();
      })()`)
      await expect.poll(async () => !(await tauriPage.isVisible(WIZARD_SELECTOR)), { timeout: 3000 }).toBeTruthy()
    })

    test(`File viewer with text file`, async ({ tauriPage }) => {
      await ensureAppReady(tauriPage)

      // Open the viewer via the production trigger (new WebviewWindow), then
      // scope axe to the new window. The viewer no longer renders into the
      // main window's `/viewer` route.
      const viewer = await openViewerWindow(tauriPage as TauriPage, testFilePath)
      const viewerLabel = viewer.targetWindow
      if (!viewerLabel) throw new Error('Scoped viewer page has no targetWindow label')

      try {
        // 3 s: viewer mounts and renders content in <1 s on a healthy machine.
        // Previous 15 s / 10 s budgets exceeded the suite's 8 s per-test ceiling.
        await viewer.waitForSelector('.viewer-container', 3000)
        await viewer.waitForSelector('.file-content', 3000)

        const { all } = await runAxeAudit(viewer, `File viewer (${mode})`)
        expect(all, `Found ${String(all.length)} violation(s) in file viewer (${mode})`).toHaveLength(0)
      } finally {
        await closeScopedWindow(tauriPage as TauriPage, viewer, viewerLabel)
      }
    })
  })
}
