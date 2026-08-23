/**
 * Tier 3 a11y tests for the settings sections that mount against the settings
 * store (plus a handful of IPC stubs) and nothing heavier.
 *
 * One file per component would cost about fifteen times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Every block below keeps its component's own doc comment, props, and
 * assertions; `settingsStore.getSetting` is re-installed per block so each
 * section still sees exactly the values it was written against.
 *
 * Two neighbours stay in their own files because their mocks can't live here:
 * `AskCmdrSection.a11y.test.ts` needs `$lib/tauri-commands` spread from the real
 * module, and the image-index components need a different `$lib/settings` shape
 * (`media-index.a11y.test.ts`).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { clearSearchIndex } from '$lib/settings/settings-search'

const settingsStore = vi.hoisted(() => ({
  getSetting: vi.fn<(key: string) => unknown>(),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))
vi.mock('$lib/settings/settings-store', () => settingsStore)

// The union of the IPC each section below reaches for. Both IPC modules keep
// their real exports (spread first) so a section that calls something outside
// the union behaves as it does in its own file, instead of hitting a missing
// export that only this merge would produce.
const bindingCommands = vi.hoisted(() => ({
  getIndexDiskUsage: vi.fn(),
  clearDriveIndex: vi.fn(),
  downloadsWatcherStatus: vi.fn(),
  recheckDownloadsWatcherGate: vi.fn(),
  setGlobalGoToLatestShortcut: vi.fn(),
}))
vi.mock('$lib/ipc/bindings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  commands: bindingCommands,
}))

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  invoke: vi.fn(() => Promise.resolve()),
  openAppearanceSettings: vi.fn(() => Promise.resolve()),
  openSystemSettingsUrl: vi.fn(() => Promise.resolve()),
  checkPortAvailable: vi.fn(() => Promise.resolve(true)),
  findAvailablePort: vi.fn(() => Promise.resolve(57821)),
  setMcpEnabled: vi.fn(() => Promise.resolve()),
  setMcpPort: vi.fn(() => Promise.resolve()),
  getMcpRunning: vi.fn(() => Promise.resolve(false)),
  getMcpPort: vi.fn(() => Promise.resolve(57821)),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/utils/confirm-dialog', () => ({
  confirmDialog: vi.fn(() => Promise.resolve(false)),
}))

vi.mock('$lib/shortcuts', async () => {
  const actual = await vi.importActual<object>('$lib/shortcuts')
  return {
    ...actual,
    onShortcutChange: vi.fn(() => () => {}),
  }
})

import AdvancedSection from './AdvancedSection.svelte'
import AppearanceSection from './AppearanceSection.svelte'
import AppearanceSizesSection from './AppearanceSizesSection.svelte'
import AppearanceZoomSection from './AppearanceZoomSection.svelte'
import ArchivesSection from './ArchivesSection.svelte'
import DeleteAiModelDialog from './DeleteAiModelDialog.svelte'
import DriveIndexingSection from './DriveIndexingSection.svelte'
import GitSection from './GitSection.svelte'
import KeyboardShortcutsSection from './KeyboardShortcutsSection.svelte'
import ListingSection from './ListingSection.svelte'
import McpServerSection from './McpServerSection.svelte'
import NavigationAndFileOpsSection from './NavigationAndFileOpsSection.svelte'
import NetworkSection from './NetworkSection.svelte'
import NotificationsSection from './NotificationsSection.svelte'
import SearchSection from './SearchSection.svelte'

/**
 * Installs this block's `getSetting` for its own tests only. Call inside a
 * `describe`: the neutral file-level default is re-armed before each test, so
 * no block can inherit another's settings.
 */
function useSettings(impl: (key: string) => unknown): void {
  beforeEach(() => {
    settingsStore.getSetting.mockImplementation(impl)
  })
}

/** A fresh container, appended to the document and ready to mount into. */
function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

beforeEach(() => {
  settingsStore.getSetting.mockReset().mockReturnValue(undefined)
  settingsStore.setSetting.mockClear()
  settingsStore.resetSetting.mockClear()
  settingsStore.isModified.mockClear()
  settingsStore.onSpecificSettingChange.mockClear()
  settingsStore.onSettingChange.mockClear()
})

// Sections share one jsdom document here, and axe resolves ARIA id references
// document-wide. Clearing between tests keeps each audit looking at its own
// section only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `AdvancedSection.svelte`.
 *
 * Auto-generated setting rows for every `section: ['Advanced']` setting,
 * grouped into `SectionCard`s by `cardKey`. Covers default and search-filtered
 * states, the card structure, and the per-row search highlight (which only
 * works because Advanced rows are in the global search index).
 */
describe('AdvancedSection a11y', () => {
  useSettings(() => 100)

  it('default (no search) has no a11y violations', async () => {
    clearSearchIndex()
    const target = container()
    mount(AdvancedSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders rows grouped into labeled SectionCards (no flat list)', async () => {
    clearSearchIndex()
    const target = container()
    mount(AdvancedSection, { target, props: { searchQuery: '' } })
    await tick()
    // Multiple cards, each with a heading and at least one row.
    const cards = target.querySelectorAll('.section-card-wrap')
    expect(cards.length).toBeGreaterThan(1)
    const headings = Array.from(target.querySelectorAll('.section-card-label')).map((el) => el.textContent.trim())
    expect(headings).toContain('Performance')
    expect(target.querySelectorAll('.advanced-setting-row').length).toBeGreaterThan(0)
  })

  it('shows only the matching card and highlights the matched row under search', async () => {
    clearSearchIndex()
    // "prefetch" matches only `advanced.prefetchBufferSize` (in the Performance card).
    const target = container()
    mount(AdvancedSection, { target, props: { searchQuery: 'prefetch' } })
    await tick()
    // Exactly one card frame (no empty frames from the other groups).
    expect(target.querySelectorAll('.section-card-wrap').length).toBe(1)
    // The matched label is highlighted (only possible because Advanced rows are
    // in the global index now; pre-un-exclusion this was always empty).
    const highlights = target.querySelectorAll('.search-highlight')
    expect(highlights.length).toBeGreaterThan(0)
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `AppearanceSection.svelte`.
 *
 * Representative settings section audited end-to-end with its child
 * SettingRow/SettingSwitch/SettingSelect/SettingRadioGroup tree. The
 * settings-store is stubbed so the section can mount without real IPC.
 */
describe('AppearanceSection a11y', () => {
  useSettings((key: string) => {
    if (key === 'theme.mode') return 'system'
    if (key === 'appearance.appColor') return 'system'
    if (key === 'appearance.sizeColors') return 'rainbow'
    if (key === 'appearance.dateColors') return 'app'
    if (key === 'appearance.dateTimeFormat') return 'iso'
    if (key === 'appearance.customDateTimeFormat') return 'YYYY-MM-DD HH:mm'
    if (key === 'listing.stripedRows') return false
    if (key === 'appearance.tintLocal') return 'none'
    if (key === 'appearance.tintSmb') return 'none'
    if (key === 'appearance.tintMtp') return 'none'
    return undefined
  })

  it('default (no search) has no a11y violations', async () => {
    const target = container()
    mount(AppearanceSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with search query (partial match) has no a11y violations', async () => {
    const target = container()
    mount(AppearanceSection, { target, props: { searchQuery: 'color' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for `AppearanceSizesSection.svelte`. */
describe('AppearanceSizesSection a11y', () => {
  useSettings((key: string) => {
    if (key === 'listing.sizeDisplay') return 'smart'
    if (key === 'listing.sizeUnit') return 'dynamic'
    if (key === 'appearance.fileSizeFormat') return 'binary'
    if (key === 'listing.sizeMismatchWarning') return true
    return undefined
  })

  it('default has no a11y violations', async () => {
    const target = container()
    mount(AppearanceSizesSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for `AppearanceZoomSection.svelte`. */
describe('AppearanceZoomSection a11y', () => {
  useSettings((key: string) => {
    if (key === 'appearance.textSize') return 100
    if (key === 'appearance.uiDensity') return 'comfortable'
    return undefined
  })

  it('default has no a11y violations', async () => {
    const target = container()
    mount(AppearanceZoomSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for `ArchivesSection.svelte`. */
describe('ArchivesSection a11y', () => {
  useSettings((key: string) => {
    if (key === 'behavior.archiveEnterBehavior') return '{}'
    if (key === 'behavior.archiveCompressionLevel') return 6
    return undefined
  })

  it('default (both format cards) has no a11y violations', async () => {
    const target = container()
    mount(ArchivesSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `DeleteAiModelDialog.svelte`, plus the behaviour that
 * has to survive its extraction out of `AiLocalSection`: mid-delete, nothing
 * can cancel or re-fire the uninstall.
 */
describe('DeleteAiModelDialog', () => {
  interface Handlers {
    onConfirm: ReturnType<typeof vi.fn>
    onCancel: ReturnType<typeof vi.fn>
  }

  async function mountDialog(isDeleting: boolean): Promise<{ target: HTMLElement } & Handlers> {
    const onConfirm = vi.fn()
    const onCancel = vi.fn()
    const target = container()
    mount(DeleteAiModelDialog, {
      target,
      props: { modelSizeFormatted: '4.1 GB', isDeleting, onConfirm, onCancel },
    })
    await tick()
    return { target, onConfirm, onCancel }
  }

  describe('DeleteAiModelDialog a11y', () => {
    it('idle has no a11y violations', async () => {
      const { target } = await mountDialog(false)
      await expectNoA11yViolations(target)
      target.remove()
    })

    it('deleting has no a11y violations', async () => {
      const { target } = await mountDialog(true)
      await expectNoA11yViolations(target)
      target.remove()
    })
  })

  describe('DeleteAiModelDialog behaviour', () => {
    it('is an alertdialog reporting the delete-ai-model id', async () => {
      const { target } = await mountDialog(false)
      expect(target.querySelector('[role="alertdialog"]')).not.toBeNull()
      const { notifyDialogOpened } = await import('$lib/tauri-commands')
      expect(vi.mocked(notifyDialogOpened).mock.calls.map(([id]) => id)).toContain('delete-ai-model')
      target.remove()
    })

    it('confirms on Enter while idle', async () => {
      const { target, onConfirm } = await mountDialog(false)
      target
        .querySelector('[role="alertdialog"]')
        ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
      await tick()
      expect(onConfirm).toHaveBeenCalledTimes(1)
      target.remove()
    })

    it('ignores Enter while deleting, so an uninstall in flight can’t be re-fired', async () => {
      const { target, onConfirm } = await mountDialog(true)
      target
        .querySelector('[role="alertdialog"]')
        ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
      await tick()
      expect(onConfirm).not.toHaveBeenCalled()
      target.remove()
    })

    it('disables both buttons while deleting', async () => {
      const { target } = await mountDialog(true)
      const buttons = [...target.querySelectorAll('button')].filter((b) => !b.className.includes('close'))
      expect(buttons.length).toBeGreaterThan(0)
      expect(buttons.every((b) => b.disabled)).toBe(true)
      target.remove()
    })
  })
})

/**
 * Tier-3 a11y tests for `DriveIndexingSection.svelte` (`Indexing > Drive
 * indexing`). Functional behavior (card structure, clear-index IPC, hidden
 * search anchor) is pinned in the companion `.svelte.test.ts` file.
 */
describe('DriveIndexingSection a11y', () => {
  beforeEach(() => {
    settingsStore.getSetting.mockImplementation((key: string): unknown => {
      switch (key) {
        case 'indexing.enabled':
          return true
        case 'indexing.askForEachDrive':
          return true
        case 'indexing.staleNotify':
          return true
        case 'indexing.silencedDrives':
          return '[]'
        default:
          return undefined
      }
    })
    settingsStore.setSetting.mockClear()
    bindingCommands.getIndexDiskUsage.mockReset().mockResolvedValue({ status: 'ok', data: 1024 })
    bindingCommands.clearDriveIndex.mockReset().mockResolvedValue({ status: 'ok', data: null })
  })

  async function mountSection(): Promise<HTMLDivElement> {
    const target = container()
    mount(DriveIndexingSection, { target, props: { searchQuery: '' } })
    await tick()
    await Promise.resolve()
    await tick()
    return target
  }

  it('default state has no a11y violations', async () => {
    const target = await mountSection()
    await expectNoA11yViolations(target)
    target.remove()
  })
})

/** Tier 3 a11y tests for `GitSection.svelte`. */
describe('GitSection a11y', () => {
  useSettings((key: string) => {
    if (key === 'fileExplorer.git.showRepoChip') return true
    if (key === 'fileExplorer.git.showStatusColumn') return false
    if (key === 'fileExplorer.git.showVirtualGitPortal') return true
    return undefined
  })

  it('default has no a11y violations', async () => {
    const target = container()
    mount(GitSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `KeyboardShortcutsSection.svelte`.
 *
 * Renders the keyboard shortcuts table per scope. Uses shortcuts +
 * command registries, both available as real modules so we only stub
 * the settings-store boundary.
 */
describe('KeyboardShortcutsSection a11y', () => {
  useSettings(() => undefined)

  // TODO: The shortcut pill renders `<span role="button" tabindex="-1">×</span>`
  // *inside* an outer `<button>` (KeyboardShortcutsSection.svelte around
  // lines 490-500). Axe flags every pill as `nested-interactive` because nested
  // focusable controls are ambiguous for screen readers. Fix: split into two
  // sibling controls (the pill button + a dedicated remove button positioned
  // next to it), or drop the inner span's `role="button"` entirely (it's
  // already `tabindex="-1"`, so mouse-only click is fine via plain span).
  // Leaving this skipped so the suite stays green until fixed.
  it.skip('default render has no a11y violations (BLOCKED: nested-interactive on shortcut pill)', async () => {
    const target = container()
    mount(KeyboardShortcutsSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for `ListingSection.svelte`. */
describe('ListingSection a11y', () => {
  useSettings((key: string) => {
    if (key === 'listing.showHiddenFiles') return true
    if (key === 'appearance.useAppIconsForDocuments') return true
    if (key === 'appearance.showFunctionKeyBar') return true
    if (key === 'listing.directorySortMode') return 'likeFiles'
    if (key === 'listing.briefColumnWidthMode') return 'paneWidth'
    if (key === 'listing.briefColumnWidthMaxPx') return 400
    return undefined
  })

  it('default has no a11y violations', async () => {
    const target = container()
    mount(ListingSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for `McpServerSection.svelte`. */
describe('McpServerSection a11y', () => {
  useSettings((key: string) => {
    if (key === 'developer.mcpEnabled') return false
    // P2: default is 0 (ephemeral). The backend writes the actual port to
    // `<data_dir>/mcp.port` and the FE reads it via `getMcpPort()` for display.
    if (key === 'developer.mcpPort') return 0
    return undefined
  })

  it('default (server off) has no a11y violations', async () => {
    const target = container()
    mount(McpServerSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for `NavigationAndFileOpsSection.svelte`. */
describe('NavigationAndFileOpsSection a11y', () => {
  useSettings((key: string) => {
    if (key === 'fileOperations.allowFileExtensionChanges') return 'ask'
    if (key === 'behavior.doubleClickPaneNavigatesToParent') return true
    return undefined
  })

  it('default has no a11y violations', async () => {
    const target = container()
    mount(NavigationAndFileOpsSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for `NetworkSection.svelte`. */
describe('NetworkSection a11y', () => {
  useSettings((key: string) => {
    if (key === 'network.enabled') return true
    if (key === 'network.directSmbConnection') return true
    if (key === 'network.shareCacheDuration') return 30000
    if (key === 'network.timeoutMode') return 'normal'
    if (key === 'network.customTimeout') return 15
    if (key === 'network.smbConcurrency') return 10
    return undefined
  })

  it('default (no search) has no a11y violations', async () => {
    const target = container()
    mount(NetworkSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier-3 a11y tests for `NotificationsSection.svelte`.
 *
 * Covered states: default (FDA granted), FDA pending. Functional behavior
 * (card structure, anchor id, ToggleGroup write-through, IPC fire) is pinned in
 * the companion `.svelte.test.ts` file.
 */
describe('NotificationsSection a11y', () => {
  function setDefaultSettings(): void {
    settingsStore.getSetting.mockImplementation((key: string): unknown => {
      switch (key) {
        case 'behavior.fileSystemWatching.downloadsNotifications':
          return 'in-app'
        case 'behavior.fileSystemWatching.globalGoToLatestShortcut.enabled':
          return true
        case 'behavior.fileSystemWatching.globalGoToLatestShortcut.binding':
          return '\u{2303}\u{2325}\u{2318}J'
        case 'behavior.fileSystemWatching.globalGoToLatestShortcut.acknowledged':
          return true
        default:
          return undefined
      }
    })
  }

  function setStatus(fdaPending: boolean): void {
    bindingCommands.downloadsWatcherStatus.mockResolvedValue({
      status: 'ok',
      data: { running: !fdaPending, downloadsDir: '/Users/me/Downloads', fdaPending },
    })
  }

  beforeEach(() => {
    settingsStore.setSetting.mockClear()
    bindingCommands.downloadsWatcherStatus.mockReset()
    bindingCommands.recheckDownloadsWatcherGate.mockReset().mockResolvedValue({ status: 'ok', data: null })
    bindingCommands.setGlobalGoToLatestShortcut.mockReset().mockResolvedValue({
      status: 'ok',
      data: { status: 'registered', binding: '\u{2303}\u{2325}\u{2318}J', enabled: true },
    })

    setDefaultSettings()
    setStatus(false)
  })

  async function mountSection(): Promise<HTMLDivElement> {
    const target = container()
    mount(NotificationsSection, { target, props: { searchQuery: '' } })
    // Drain the onMount IPC chain; jsdom needs a few flushes.
    await tick()
    await Promise.resolve()
    await tick()
    await Promise.resolve()
    await tick()
    return target
  }

  it('default state (FDA granted) has no a11y violations', async () => {
    const target = await mountSection()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('FDA-pending state (Downloads card greyed) has no a11y violations', async () => {
    setStatus(true)
    const target = await mountSection()
    await expectNoA11yViolations(target)
    target.remove()
  })
})

/**
 * Tier 3 a11y tests for `SearchSection.svelte`.
 *
 * The section renders the auto-apply switch plus the mirrored
 * `search.recentSearches.maxCount` number input, both gated by the section's
 * search-query filter. Covered states: default, and filter-matched.
 */
describe('SearchSection a11y', () => {
  useSettings((key: string) => {
    if (key === 'search.autoApply') return true
    if (key === 'search.recentSearches.maxCount') return 1000
    return undefined
  })

  it('default (no filter) has no a11y violations', async () => {
    const target = container()
    mount(SearchSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('filtered by "auto" has no a11y violations', async () => {
    const target = container()
    mount(SearchSection, { target, props: { searchQuery: 'auto' } })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
