/**
 * The surfaces translators asked to see, for the i18n screenshot-capture driver
 * (`i18n-capture.spec.ts`): explorer toasts, the quick filter, the Multi-rename sheet, servers, and
 * Settings under a managed policy.
 *
 * Where a surface's trigger is a backend state no fixture tree produces (a server wanting an
 * account, macOS's stored Automation refusal, an S3 price), its one IPC answer is stubbed
 * (`i18n-capture-ipc-stub.ts`) and the frontend's real code renders the rest.
 *
 * A step here leaves the shared app as it found it, like every step of the main pass: the panes
 * back on the fixture tree, stubs restored, and no toast or policy left behind.
 */

import fs from 'node:fs'
import { waitBudget } from './wait-budget.js'
import { expect } from './fixtures.js'
import {
  closeScopedWindow,
  dismissAllToasts,
  dismissOverlay,
  dispatchMenuCommand,
  emitBackendEvent,
  ensureAppReady,
  escapeOverlayUntilGone,
  getFixtureRoot,
  moveCursorToFile,
  openSettingsWindowViaProd,
  skipParentEntry,
} from './helpers.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { initMcpClient, mcpSelectVolume } from '../e2e-shared/mcp-client.js'
import { selectAll } from './conflict-helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import {
  type SurfaceEntry,
  captureCall,
  captureSurface,
  captureToastSurface,
  focusWindow,
} from './i18n-capture-helpers.js'
import { stubIpcAnswer } from './i18n-capture-ipc-stub.js'
import { openGalleryState } from './dialog-gallery-helpers.js'
import { removePolicyFileAndWait, waitForManagedPolicy, writePolicyFile } from './managed-policy-helpers.js'

type Report = Record<string, SurfaceEntry>

const LOCAL_VOLUME_NAME = 'Macintosh HD'

/** Starts a main-window surface's sink. */
async function startSink(page: TauriPage, label: string): Promise<void> {
  await captureCall(page, 'reset')
  await captureCall(page, 'setSurface', label)
  await captureCall<boolean>(page, 'enable')
}

/** Puts a pane on `path`, through the same event MCP's navigation uses, and waits for it to show (or not show) `file`. */
async function navigatePane(
  main: TauriPage,
  pane: 'left' | 'right',
  path: string,
  file: string,
  shown: boolean,
): Promise<void> {
  await emitBackendEvent(main, 'mcp-nav-to-path', { pane, path })
  await expect
    .poll(
      () =>
        main.evaluate<boolean>(
          `document.querySelectorAll('.file-pane')[${pane === 'left' ? '0' : '1'}]?.querySelector('.file-entry[data-filename="${file}"]') !== null`,
        ),
      { timeout: waitBudget(5000) },
    )
    .toBe(shown)
}

// ── Servers ─────────────────────────────────────────────────────────────────

const NEARBY_HOST = {
  id: 'capture-nearby-host',
  name: 'Naspolya',
  hostname: 'naspolya.local',
  ipAddress: '192.168.1.20',
  port: 445,
  source: 'discovered',
}

/**
 * The Servers hub with a host only found nearby, then that host's sign-in screen: the state a share
 * list falls to when a server turns a guest away. Only with its sheet up: cancelling the sheet walks
 * back to the hub by design, so the screen is never on show without it. Discovery announces through events nothing in Rust
 * listens to, and the host goes away again with its own `network-host-lost`.
 */
export async function captureNetworkSurfaces(main: TauriPage, report: Report, failed: string[]): Promise<void> {
  await ensureAppReady(main)
  await initMcpClient(main)
  const restore = await stubIpcAnswer(main, 'list_shares_on_host', {
    err: { type: 'auth_required', message: 'Scripted guest refusal' },
  })
  try {
    await mcpSelectVolume('left', 'Servers')
    await main.waitForSelector('.servers-hub .add-row', waitBudget(15000))
    await emitBackendEvent(main, 'network-host-found', NEARBY_HOST)
    const group = '.servers-hub .nearby-group-label'

    await captureSurface('servers-hub-nearby', report, failed, async () => {
      await startSink(main, 'servers-hub-nearby')
      await main.waitForSelector(group, waitBudget(5000))
      return { page: main, readySelector: group }
    })
    await captureCall(main, 'disable').catch(() => {})

    await captureSurface('network-sign-in-sheet', report, failed, async () => {
      await startSink(main, 'network-sign-in-sheet')
      // Unfold the group if it's folded, then open the host, as a double click would.
      await main.evaluate(`(function(){
        var toggle = document.querySelector('.servers-hub .nearby-group-label')?.closest('button');
        if (toggle && toggle.getAttribute('aria-expanded') !== 'true') toggle.click();
      })()`)
      await main.waitForSelector('.servers-hub [data-hub-row] .row-icon', waitBudget(5000))
      await expect
        .poll(
          async () => {
            await main.evaluate(`(function(){
              var rows = [...document.querySelectorAll('.servers-hub [data-hub-row]')];
              var row = rows.find(function(r){ return (r.textContent || '').includes(${JSON.stringify(NEARBY_HOST.name)}) && !r.querySelector('.nearby-group-label'); });
              if (row) row.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
            })()`)
            return main.evaluate<boolean>(`document.querySelector('.sign-in-state') !== null`)
          },
          { timeout: waitBudget(5000) },
        )
        .toBe(true)
      await main.waitForSelector('[data-dialog-id="server-sign-in"]', waitBudget(5000))
      return { page: main, readySelector: '[data-dialog-id="server-sign-in"]' }
    })
    await captureCall(main, 'disable').catch(() => {})
  } finally {
    await restore()
    await emitBackendEvent(main, 'network-host-lost', { id: NEARBY_HOST.id }).catch(() => {})
    await dismissOverlay(main).catch(() => {})
    await mcpSelectVolume('left', LOCAL_VOLUME_NAME).catch(() => {})
  }
}

/**
 * The sign-in sheet's refusals that take a press to reach: an Add answered with the Mac's own Local
 * Network switch, and an S3 Add answered with a bucket in another region. The gallery row's `attempt`
 * is a fixture, so nothing dials.
 */
export async function captureServerSheetRefusals(main: TauriPage, report: Report, failed: string[]): Promise<void> {
  await ensureAppReady(main)
  const staged = [
    { label: 'server-sign-in-refused-local-network', state: 'add-local-network', secret: 'p4ssword' },
    { label: 'server-sign-in-refused-s3', state: 'add-s3', secret: 'wJalrXUtnFEMIEXAMPLEKEY' },
  ]
  for (const { label, state, secret } of staged) {
    await captureSurface(label, report, failed, async () => {
      await startSink(main, label)
      await openGalleryState(main, 'server-sign-in', state, null)
      const sheet = '[data-dialog-id="server-sign-in"]'
      await main.waitForSelector(`${sheet} input`, waitBudget(5000))
      await main.evaluate(`(function(){
        document.querySelectorAll('${sheet} input[type="password"]').forEach(function(field){
          field.value = ${JSON.stringify(secret)};
          field.dispatchEvent(new Event('input', { bubbles: true }));
        });
      })()`)
      // Pressed once: the fixture answers after a beat, and a second press would clear the answer.
      const submit = `${sheet} .btn-primary:not([disabled])`
      await main.waitForSelector(submit, waitBudget(5000))
      await main.evaluate(`document.querySelector('${submit}')?.click()`)
      await main.waitForSelector(`${sheet} [role="alert"]`, waitBudget(5000))
      return { page: main, readySelector: `${sheet} [role="alert"]` }
    })
    await captureCall(main, 'disable').catch(() => {})
    await dismissOverlay(main).catch(() => {})
  }
}

// ── Explorer toasts ─────────────────────────────────────────────────────────

/**
 * Compare directories' answers: differences marked, none found, and a pane with no folder to
 * compare. The right pane starts empty, so the first marks the left pane's files.
 */
export async function captureCompareToasts(main: TauriPage, report: Report, failed: string[]): Promise<void> {
  recreateFixtures(getFixtureRoot())
  await ensureAppReady(main)
  await initMcpClient(main)
  const left = `${getFixtureRoot()}/left`
  const right = `${getFixtureRoot()}/right`
  await captureToastSurface('toast-compare-marked', report, failed, main, async () => {
    await dispatchMenuCommand(main, 'selection.compareDirectories')
  })
  await clearMarks(main)
  await navigatePane(main, 'right', left, 'file-a.txt', true)
  await captureToastSurface('toast-compare-no-differences', report, failed, main, async () => {
    await dispatchMenuCommand(main, 'selection.compareDirectories')
  })
  await navigatePane(main, 'right', right, 'file-a.txt', false)
  await mcpSelectVolume('left', 'Servers')
  await main.waitForSelector('.servers-hub .add-row', waitBudget(15000))
  await captureToastSurface('toast-compare-needs-two-folders', report, failed, main, async () => {
    await dispatchMenuCommand(main, 'selection.compareDirectories')
  })
  await mcpSelectVolume('left', LOCAL_VOLUME_NAME).catch(() => {})
  await navigatePane(main, 'left', left, 'file-a.txt', true)
}

/** Copy share link on a local file, which says where it works. The link itself needs S3. */
export async function captureShareLinkToast(main: TauriPage, report: Report, failed: string[]): Promise<void> {
  await ensureAppReady(main)
  await captureToastSurface('toast-share-link-unavailable', report, failed, main, async () => {
    await dispatchMenuCommand(main, 'file.copyShareLink')
  })
}

/** Clears the selection a comparison left, through the same command the menu has. */
async function clearMarks(main: TauriPage): Promise<void> {
  await dispatchMenuCommand(main, 'selection.deselectAll')
  await expect
    .poll(() => main.evaluate<number>(`document.querySelectorAll('.is-selected').length`), {
      timeout: waitBudget(3000),
    })
    .toBe(0)
}

/** Calculate folder sizes on a volume that went away mid-way. */
export async function captureFolderSizesToast(main: TauriPage, report: Report, failed: string[]): Promise<void> {
  await ensureAppReady(main)
  const restore = await stubIpcAnswer(main, 'count_folder_sizes', { err: { type: 'notConnected' } })
  try {
    await captureToastSurface('toast-folder-sizes-not-connected', report, failed, main, async () => {
      await dispatchMenuCommand(main, 'view.calculateFolderSizes')
    })
  } finally {
    await restore()
  }
}

/** Get info's two refusals: Automation turned off for Cmdr, and Finder not answering. */
export async function captureGetInfoToasts(main: TauriPage, report: Report, failed: string[]): Promise<void> {
  await ensureAppReady(main)
  await skipParentEntry(main)
  const refusals = [
    { label: 'toast-get-info-automation-off', err: { type: 'automationDenied' } },
    { label: 'toast-get-info-not-reached', err: { type: 'timedOut' } },
  ]
  for (const { label, err } of refusals) {
    const restore = await stubIpcAnswer(main, 'get_info', { err })
    try {
      await captureToastSurface(label, report, failed, main, async () => {
        await dispatchMenuCommand(main, 'file.getInfo')
      })
    } finally {
      await restore()
    }
  }
}

// ── Quick filter ────────────────────────────────────────────────────────────

/** Types into the focused pane through the key handler the list reads. */
async function typeInPane(main: TauriPage, key: string): Promise<void> {
  await main.evaluate(`(function(){
    var target = document.querySelector('.dual-pane-explorer') || document.body;
    target.dispatchEvent(new KeyboardEvent('keydown', { key: ${JSON.stringify(key)}, code: 'Key' + ${JSON.stringify(key)}.toUpperCase(), bubbles: true }));
  })()`)
}

/**
 * The first quick filter's intro toast, the badge with its ×, and the Jump badge ⌃S flips it to. The
 * intro shows once per install, so the seen-flag goes back to unseen first (a lane shard reaches
 * this after other specs have typed), and stays seen after.
 */
export async function captureQuickFilterSurfaces(main: TauriPage, report: Report, failed: string[]): Promise<void> {
  recreateFixtures(getFixtureRoot())
  await ensureAppReady(main)
  await main.waitForSelector('.file-pane .file-entry[data-filename="file-a.txt"]', waitBudget(5000))
  await emitBackendEvent(main, 'mcp-set-setting', {
    requestId: 'i18n-capture',
    settingId: 'fileExplorer.quickFilterIntroSeen',
    value: false,
  })
  try {
    await captureToastSurface('quick-filter-intro', report, failed, main, async () => {
      await expect
        .poll(
          async () => {
            await typeInPane(main, 'f')
            return main.evaluate<boolean>(`document.querySelector('.toast') !== null`)
          },
          { timeout: waitBudget(5000) },
        )
        .toBe(true)
    })

    const badge = '.type-to-jump-indicator .clear'
    await captureSurface('quick-filter-badge', report, failed, async () => {
      await startSink(main, 'quick-filter-badge')
      await main.waitForSelector(badge, waitBudget(5000))
      return { page: main, readySelector: badge }
    })
    await captureCall(main, 'disable').catch(() => {})

    // ⌃S flips the badge between Filter and Jump, and says which through the pane's live region, so
    // both words get recorded on the way to the one this leaves up. Re-dispatching inside a poll
    // because the cross-source guard (`dispatch-dedup.ts`, 300 ms) can swallow a press.
    await captureSurface('quick-filter-mode', report, failed, async () => {
      await startSink(main, 'quick-filter-mode')
      const badgeText = () =>
        main.evaluate<string>(`document.querySelector('.type-to-jump-indicator')?.textContent || ''`)
      const filterBadge = await badgeText()
      await expect
        .poll(
          async () => {
            await dispatchMenuCommand(main, 'pane.quickFilter')
            return (await badgeText()) !== filterBadge
          },
          { timeout: waitBudget(5000) },
        )
        .toBe(true)
      return { page: main, readySelector: '.type-to-jump-indicator' }
    })
    await captureCall(main, 'disable').catch(() => {})
  } finally {
    await main.evaluate(`document.querySelector('.type-to-jump-indicator .clear')?.click()`).catch(() => {})
    await dismissAllToasts(main).catch(() => {})
    await expect
      .poll(() => main.evaluate<number>(`document.querySelectorAll('.type-to-jump-indicator').length`), {
        timeout: waitBudget(3000),
      })
      .toBe(0)
      .catch(() => {})
  }
}

// ── Favorites ───────────────────────────────────────────────────────────────

/**
 * A favorite whose folder is gone: its dimmed row with the status under its path, then the toast a
 * pick raises, with its Remove button. The folder is made here and deleted, so the favorite really
 * is stale, and the favorite goes away again at the end.
 */
export async function captureFavoriteUnreachable(main: TauriPage, report: Report, failed: string[]): Promise<void> {
  await ensureAppReady(main)
  const gone = fs.mkdtempSync(`${getFixtureRoot()}/capture-favorite-`)
  await main.evaluate(
    `window.__TAURI_INTERNALS__.invoke('add_favorite', { path: ${JSON.stringify(gone)}, name: 'Old holiday photos' })`,
  )
  fs.rmSync(gone, { recursive: true })
  const dimmed = '[data-menu] .favorite-label.is-unreachable'
  const openFavoritesMenu = () => emitBackendEvent(main, 'execute-command', { commandId: 'favorites.open' })
  try {
    await captureSurface('favorites-menu-unreachable', report, failed, async () => {
      await startSink(main, 'favorites-menu-unreachable')
      await openFavoritesMenu()
      await main.waitForSelector(dimmed, waitBudget(5000))
      return { page: main, readySelector: dimmed }
    })
    await captureCall(main, 'disable').catch(() => {})

    await captureToastSurface('toast-favorite-unreachable', report, failed, main, async () => {
      await main.waitForSelector(dimmed, waitBudget(5000))
      await main.evaluate(`document.querySelector('${dimmed}')?.closest('[data-menu-row]')?.click()`)
    })
  } finally {
    await main
      .evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`)
      .catch(() => {})
    await main
      .evaluate(`(async function(){
        var res = await window.__TAURI_INTERNALS__.invoke('list_volumes');
        var row = res.data.find(function(v){ return v.category === 'favorite' && v.path === ${JSON.stringify(gone)}; });
        if (row) await window.__TAURI_INTERNALS__.invoke('remove_favorite', { id: row.id.replace(/^fav-/, '') });
      })()`)
      .catch(() => {})
  }
}

// ── Multi-rename ────────────────────────────────────────────────────────────

const SHEET = '[data-dialog-id="multi-rename"]'
const PRESET_NAME = 'Holiday photos'
const PRESET_DELETE = '[data-menu-row$=":delete"]'

/** The sheet's Presets button, which the sheet's own F2 and File > Rename also toggle. */
function clickPresetsButtonJs(): string {
  return `document.querySelector('${SHEET} .presets button')?.click()`
}

/** The Multi-rename sheet on two selected files, its Presets menu, a preset's submenu, and the name popover. */
export async function captureMultiRenameSurfaces(main: TauriPage, report: Report, failed: string[]): Promise<void> {
  recreateFixtures(getFixtureRoot())
  await ensureAppReady(main)
  await selectAll(main)
  try {
    await captureSurface('multi-rename', report, failed, async () => {
      await startSink(main, 'multi-rename')
      await dispatchMenuCommand(main, 'file.multiRename')
      await main.waitForSelector(`${SHEET} .sheet`, waitBudget(5000))
      return { page: main, readySelector: `${SHEET} .sheet`, fitSelector: SHEET }
    })
    await captureCall(main, 'disable').catch(() => {})

    await captureSurface('multi-rename-preset-name', report, failed, async () => {
      await startSink(main, 'multi-rename-preset-name')
      await main.evaluate(clickPresetsButtonJs())
      await main.waitForSelector('[data-menu] [data-menu-row]', waitBudget(5000))
      // Save as preset… is the menu's last row.
      await main.evaluate(`(function(){
        var rows = document.querySelectorAll('[data-menu] [data-menu-row]');
        rows[rows.length - 1].click();
      })()`)
      await main.waitForSelector('.name-form input', waitBudget(5000))
      return { page: main, readySelector: '.name-form input' }
    })
    await captureCall(main, 'disable').catch(() => {})
    await main.evaluate(`(function(){
      var field = document.querySelector('.name-form input');
      field.value = ${JSON.stringify(PRESET_NAME)};
      field.dispatchEvent(new Event('input', { bubbles: true }));
      field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    })()`)
    await expect
      .poll(() => main.evaluate<boolean>(`document.querySelector('${SHEET} .loaded-name') !== null`), {
        timeout: waitBudget(5000),
      })
      .toBe(true)

    await captureSurface('multi-rename-presets', report, failed, async () => {
      await startSink(main, 'multi-rename-presets')
      await main.evaluate(clickPresetsButtonJs())
      await main.waitForSelector('[data-menu] [data-menu-row]', waitBudget(5000))
      return { page: main, readySelector: '[data-menu] [data-menu-row]' }
    })
    await captureCall(main, 'disable').catch(() => {})

    await captureSurface('multi-rename-preset-submenu', report, failed, async () => {
      await startSink(main, 'multi-rename-preset-submenu')
      // The keyboard's way in: highlight the row (the menu ignores a pointer that didn't move, so
      // it moves twice), then the right arrow opens its submenu.
      await expect
        .poll(
          async () => {
            await main.evaluate(`(function(){
              var row = [...document.querySelectorAll('[data-menu] [data-menu-row]')]
                .find(function(r){ return (r.textContent || '').includes(${JSON.stringify(PRESET_NAME)}); });
              var box = row.getBoundingClientRect();
              [4, 8].forEach(function(nudge){
                row.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, clientX: box.left + nudge, clientY: box.top + nudge }));
              });
              document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', code: 'ArrowRight', bubbles: true }));
            })()`)
            return main.evaluate<boolean>(`document.querySelector('${PRESET_DELETE}') !== null`)
          },
          { timeout: waitBudget(5000) },
        )
        .toBe(true)
      return { page: main, readySelector: PRESET_DELETE }
    })
    await captureCall(main, 'disable').catch(() => {})
  } finally {
    // The preset goes back out of the shard's store through the submenu's own Delete.
    await main.evaluate(`document.querySelector('${PRESET_DELETE}')?.click()`).catch(() => {})
    await escapeOverlayUntilGone(main, SHEET).catch(() => {})
  }
}

// ── Listing stalled ─────────────────────────────────────────────────────────

/**
 * The pane's "still waiting" screen. A local listing finishes in milliseconds, so the listing's start
 * is stubbed to answer without ever streaming, which keeps the pane loading for as long as the shot
 * takes, and the event a hung mount sends is emitted for it. Nothing in Rust listens to that event.
 */
export async function captureListingStalled(main: TauriPage, report: Report, failed: string[]): Promise<void> {
  recreateFixtures(getFixtureRoot())
  await ensureAppReady(main)
  const left = `${getFixtureRoot()}/left`
  const restore = await stubIpcAnswer(main, 'list_directory_start_streaming', {
    ok: { listingId: 'capture-stalled', status: 'loading' },
  })
  try {
    await captureSurface('listing-stalled', report, failed, async () => {
      await startSink(main, 'listing-stalled')
      await emitBackendEvent(main, 'mcp-nav-to-path', { pane: 'left', path: `${left}/sub-dir` })
      await expect
        .poll(
          async () => {
            const listingId = await main.evaluate<string>(
              `document.querySelector('.file-pane')?.dataset.listingId || ''`,
            )
            if (listingId !== '') await emitBackendEvent(main, 'listing-stalled', { listingId, stalledOn: 'server' })
            return main.evaluate<boolean>(`document.querySelector('.stalled .actions') !== null`)
          },
          { timeout: waitBudget(5000) },
        )
        .toBe(true)
      return { page: main, readySelector: '.stalled .actions' }
    })
    await captureCall(main, 'disable').catch(() => {})
  } finally {
    await restore()
    await emitBackendEvent(main, 'mcp-nav-to-path', { pane: 'left', path: left }).catch(() => {})
    await main.waitForSelector('.file-entry', waitBudget(5000)).catch(() => {})
  }
}

// ── S3 cost line ────────────────────────────────────────────────────────────

/** The price line under a Copy's scan, as the backend would price it for a place on S3. */
export async function captureS3CostLine(main: TauriPage, report: Report, failed: string[]): Promise<void> {
  recreateFixtures(getFixtureRoot())
  await ensureAppReady(main)
  const restore = await stubIpcAnswer(main, 'estimate_operation_cost', {
    ok: [{ amount: 0.42, currency: 'USD', providerLabel: 'AWS' }],
  })
  try {
    await captureSurface('transfer-s3-cost', report, failed, async () => {
      await startSink(main, 'transfer-s3-cost')
      await skipParentEntry(main)
      await moveCursorToFile(main, 'file-a.txt')
      await dispatchMenuCommand(main, 'file.copy')
      await main.waitForSelector('.s3-cost', waitBudget(10000))
      return { page: main, readySelector: '.s3-cost' }
    })
    await captureCall(main, 'disable').catch(() => {})
  } finally {
    await restore()
    await dismissOverlay(main).catch(() => {})
  }
}

// ── Managed by an organization ──────────────────────────────────────────────

const MANAGED_POLICY = `  <key>DisableUsageStats</key><true/>
  <key>DisableCrashAndErrorReports</key><true/>
  <key>DisableUpdates</key><true/>
  <key>DisableCloudAI</key><true/>`

/**
 * Settings under a policy: the managed card with its locked rows, and the AI provider's refusal. The
 * policy goes through the file `CMDR_MANAGED_PREFS_FILE` names (the lane sets it per shard, the
 * capture's orchestrator per run), and is gone again before this returns, or every later spec on
 * the shard would inherit it.
 */
export async function captureManagedSurfaces(main: TauriPage, report: Report, failed: string[]): Promise<void> {
  const policyFile = process.env.CMDR_MANAGED_PREFS_FILE
  if (policyFile === undefined) throw new Error('CMDR_MANAGED_PREFS_FILE is unset: no policy can reach the app')
  await ensureAppReady(main)
  let settings: TauriPage | undefined
  writePolicyFile(policyFile, MANAGED_POLICY)
  try {
    await waitForManagedPolicy(main)
    settings = await openSettingsWindowViaProd(main)
    const window = settings
    // Same wait as the Settings capture: its content gates on `initialized`, which the sidebar proves.
    await window.waitForSelector('.settings-window', 5000)
    await focusWindow(window, 'settings')
    await window.waitForSelector('.settings-window .section-item', 10000)
    const sections = [
      {
        label: 'settings-updates-managed',
        path: ['Updates & privacy'],
        ready: '[data-section-id="updates"] [id$="-disabled-note"]',
      },
      {
        label: 'settings-ai-provider-managed',
        path: ['AI', 'Provider'],
        ready: '#settings-ai-provider-managed',
      },
    ]
    for (const { label, path, ready } of sections) {
      await captureSurface(label, report, failed, async () => {
        await startSink(window, label)
        await window.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit_to', {
          target: { kind: 'WebviewWindow', label: 'settings' },
          event: 'navigate-to-section',
          payload: ${JSON.stringify({ section: path })}
        })`)
        await window.waitForSelector(ready, waitBudget(5000))
        return { page: window, focusLabel: 'settings', readySelector: ready }
      })
      await captureCall(window, 'disable').catch(() => {})
    }
  } finally {
    if (settings) await closeScopedWindow(main, settings, 'settings').catch(() => {})
    await removePolicyFileAndWait(main, policyFile)
  }
}
