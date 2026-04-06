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
  ensureAppReady,
  navigateToRoute,
  executeViaCommandPalette,
  moveCursorToFile,
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

// Use fixture file from the shared E2E fixture tree
const fixtureRoot = process.env.CMDR_E2E_START_PATH ?? '/tmp/cmdr-e2e-fallback'
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

  const axeCall = scope ? `axe.run(document.querySelector(${JSON.stringify(scope)}))` : 'axe.run()'
  const results = await tauriPage.evaluate<AxeResults>(axeCall)

  const critical = results.violations.filter((v) => v.impact === 'critical')
  const serious = results.violations.filter((v) => v.impact === 'serious')
  const moderate = results.violations.filter((v) => v.impact === 'moderate')
  const minor = results.violations.filter((v) => v.impact === 'minor')

  // Log all violations for visibility
  for (const v of results.violations) {
    // eslint-disable-next-line no-console
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
    // eslint-disable-next-line no-console
    console.log(`\n⚠ [${viewName}] ${counts} violation(s) found`)
  } else {
    // eslint-disable-next-line no-console
    console.log(`✓ [${viewName}] No accessibility violations`)
  }

  return { critical, serious, moderate, minor, all: results.violations }
}

/** Dismiss a modal dialog with Escape and wait for it to close. */
async function dismissDialog(tauriPage: PageLike): Promise<void> {
  await tauriPage.keyboard.press('Escape')
  await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 5000)
}

/** Open the command palette overlay. */
async function openCommandPalette(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', {
        key: 'p', ctrlKey: ${String(CTRL_OR_META === 'Control')}, metaKey: ${String(CTRL_OR_META === 'Meta')}, shiftKey: true, bubbles: true
    }))`)
  await tauriPage.waitForSelector('.palette-overlay', 5000)
}

/** Open the search dialog overlay. */
async function openSearchDialog(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', {
        key: 'f', ctrlKey: ${String(CTRL_OR_META === 'Control')}, metaKey: ${String(CTRL_OR_META === 'Meta')}, bubbles: true
    }))`)
  await tauriPage.waitForSelector('.search-overlay', 5000)
}

/** Switch the app theme via Tauri's setTheme API. */
async function setTheme(tauriPage: PageLike, mode: 'dark' | 'light'): Promise<void> {
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:app|set_app_theme', { theme: '${mode}' })`)
  await sleep(500) // Let the theme transition settle
}

// ── Tests ───────────────────────────────────────────────────────────────────

for (const mode of ['light', 'dark'] as const) {
  test.describe(`${mode} mode`, () => {
    test.beforeEach(async ({ tauriPage }) => {
      await setTheme(tauriPage, mode)
    })

    test(`main explorer view`, async ({ tauriPage }) => {
      test.setTimeout(120_000)
      await ensureAppReady(tauriPage)

      const { all } = await runAxeAudit(tauriPage, `Main explorer (${mode})`)
      expect(all, `Found ${String(all.length)} violation(s) in main explorer (${mode})`).toHaveLength(0)
    })

    test(`Copy dialog`, async ({ tauriPage }) => {
      test.setTimeout(120_000)
      await ensureAppReady(tauriPage)
      await moveCursorToFile(tauriPage, 'file-a.txt')

      await tauriPage.keyboard.press('F5')
      await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

      const { all } = await runAxeAudit(tauriPage, `Copy dialog (${mode})`, TRANSFER_DIALOG)
      await dismissDialog(tauriPage)
      expect(all, `Found ${String(all.length)} violation(s) in Copy dialog (${mode})`).toHaveLength(0)
    })

    test(`Delete dialog`, async ({ tauriPage }) => {
      test.setTimeout(120_000)
      await ensureAppReady(tauriPage)
      await moveCursorToFile(tauriPage, 'file-a.txt')

      await tauriPage.keyboard.press('F8')
      const deleteDialog = '[data-dialog-id="delete-confirmation"]'
      await tauriPage.waitForSelector(deleteDialog, 5000)

      const { all } = await runAxeAudit(tauriPage, `Delete dialog (${mode})`, deleteDialog)
      await dismissDialog(tauriPage)
      expect(all, `Found ${String(all.length)} violation(s) in Delete dialog (${mode})`).toHaveLength(0)
    })

    test(`Move dialog`, async ({ tauriPage }) => {
      test.setTimeout(120_000)
      await ensureAppReady(tauriPage)
      await moveCursorToFile(tauriPage, 'file-a.txt')

      await tauriPage.keyboard.press('F6')
      await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

      const { all } = await runAxeAudit(tauriPage, `Move dialog (${mode})`, TRANSFER_DIALOG)
      await dismissDialog(tauriPage)
      expect(all, `Found ${String(all.length)} violation(s) in Move dialog (${mode})`).toHaveLength(0)
    })

    test(`About dialog`, async ({ tauriPage }) => {
      test.setTimeout(120_000)
      await ensureAppReady(tauriPage)

      await executeViaCommandPalette(tauriPage, 'About Cmdr')
      await tauriPage.waitForSelector('[data-dialog-id="about"]', 5000)

      const { all } = await runAxeAudit(tauriPage, `About dialog (${mode})`, '[data-dialog-id="about"]')
      await dismissDialog(tauriPage)
      expect(all, `Found ${String(all.length)} violation(s) in About dialog (${mode})`).toHaveLength(0)
    })

    test(`License dialog`, async ({ tauriPage }) => {
      test.setTimeout(120_000)
      await ensureAppReady(tauriPage)

      await executeViaCommandPalette(tauriPage, 'license')
      await tauriPage.waitForSelector('[data-dialog-id="license"]', 5000)

      const { all } = await runAxeAudit(tauriPage, `License dialog (${mode})`, '[data-dialog-id="license"]')
      await dismissDialog(tauriPage)
      expect(all, `Found ${String(all.length)} violation(s) in License dialog (${mode})`).toHaveLength(0)
    })

    test(`Command palette`, async ({ tauriPage }) => {
      test.setTimeout(120_000)
      await ensureAppReady(tauriPage)

      await openCommandPalette(tauriPage)

      const { all } = await runAxeAudit(tauriPage, `Command palette (${mode})`, '.palette-overlay')

      // Dismiss the palette
      await tauriPage.keyboard.press('Escape')
      await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.palette-overlay')), 3000)

      expect(all, `Found ${String(all.length)} violation(s) in command palette (${mode})`).toHaveLength(0)
    })

    test(`Search dialog`, async ({ tauriPage }) => {
      test.setTimeout(120_000)
      await ensureAppReady(tauriPage)

      await openSearchDialog(tauriPage)

      const { all } = await runAxeAudit(tauriPage, `Search dialog (${mode})`, '.search-overlay')

      // Dismiss the search dialog
      await tauriPage.keyboard.press('Escape')
      await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.search-overlay')), 3000)

      expect(all, `Found ${String(all.length)} violation(s) in search dialog (${mode})`).toHaveLength(0)
    })

    test(`Settings: all sections`, async ({ tauriPage }) => {
      test.setTimeout(180_000)
      await ensureAppReady(tauriPage)

      // Navigate to settings
      await navigateToRoute(tauriPage, '/settings')
      await tauriPage.waitForSelector('.settings-window', 15000)
      await tauriPage.waitForSelector('.settings-sidebar', 10000)

      // All settings sections with their sidebar paths and data-section-id selectors
      const sections: { name: string; path: string[]; sectionId: string }[] = [
        { name: 'Appearance', path: ['General', 'Appearance'], sectionId: 'general-appearance' },
        { name: 'Listing', path: ['General', 'Listing'], sectionId: 'general-listing' },
        { name: 'File operations', path: ['General', 'File operations'], sectionId: 'general-file-operations' },
        { name: 'Drive indexing', path: ['General', 'Drive indexing'], sectionId: 'general-drive-indexing' },
        { name: 'Updates', path: ['General', 'Updates'], sectionId: 'general-updates' },
        { name: 'Viewer', path: ['General', 'Viewer'], sectionId: 'general-viewer' },
        {
          name: 'SMB/Network shares',
          path: ['Network', 'SMB/Network shares'],
          sectionId: 'network-smb-network-shares',
        },
        { name: 'Keyboard shortcuts', path: ['Keyboard shortcuts'], sectionId: 'keyboard-shortcuts' },
        { name: 'Themes', path: ['Themes'], sectionId: 'themes' },
        { name: 'License', path: ['License'], sectionId: 'license' },
        { name: 'AI', path: ['AI'], sectionId: 'ai' },
        { name: 'MCP server', path: ['Developer', 'MCP server'], sectionId: 'developer-mcp-server' },
        { name: 'Logging', path: ['Developer', 'Logging'], sectionId: 'developer-logging' },
        { name: 'Advanced', path: ['Advanced'], sectionId: 'advanced' },
      ]

      const allViolations: { section: string; violations: AxeViolation[] }[] = []

      for (const section of sections) {
        // Click sidebar to navigate to the section
        await tauriPage.evaluate(`(function() {
            var items = document.querySelectorAll('.section-item');
            for (var i = 0; i < items.length; i++) {
                if (items[i].textContent.trim() === ${JSON.stringify(section.path[section.path.length - 1])}) {
                    items[i].click();
                    break;
                }
            }
        })()`)
        await sleep(500)

        // Wait for the section to be visible
        const sectionSelector = `[data-section-id="${section.sectionId}"]`
        const sectionVisible = await pollUntil(tauriPage, async () => tauriPage.isVisible(sectionSelector), 5000)
        if (!sectionVisible) {
          // eslint-disable-next-line no-console
          console.log(`⚠ Settings section "${section.name}" not visible, skipping`)
          continue
        }

        const { all } = await runAxeAudit(tauriPage, `Settings: ${section.name} (${mode})`)
        if (all.length > 0) {
          allViolations.push({ section: section.name, violations: all })
        }
      }

      const totalViolations = allViolations.reduce((sum, s) => sum + s.violations.length, 0)
      const failedSections = allViolations.map((s) => `${s.section} (${String(s.violations.length)})`).join(', ')
      expect(totalViolations, `Violations in settings (${mode}): ${failedSections}`).toBe(0)
    })

    test(`File viewer with text file`, async ({ tauriPage }) => {
      test.setTimeout(120_000)
      await ensureAppReady(tauriPage)

      // Navigate to viewer with the ~1KB text file
      const viewerPath = `/viewer?path=${encodeURIComponent(testFilePath)}`
      await navigateToRoute(tauriPage, viewerPath)
      await tauriPage.waitForSelector('.viewer-container', 15000)
      await tauriPage.waitForSelector('.file-content', 10000)

      const { all } = await runAxeAudit(tauriPage, `File viewer (${mode})`)
      expect(all, `Found ${String(all.length)} violation(s) in file viewer (${mode})`).toHaveLength(0)
    })
  })
}
