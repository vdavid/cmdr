/**
 * Tier-3 tests for `NavigationAndFileOpsSection.svelte`
 * (Behavior › Navigation & file ops).
 *
 * Labeled cards: "Navigation" (the double-click-to-parent switch), "File
 * operations" (the file-extension-change radio), "Text editor" (which app F4
 * opens files in) and "Terminal" (which app "Open terminal here" launches), both
 * macOS-only, and "Operation log" (the retention limits). The conflict/progress
 * settings live in Advanced (their single home), never mirrored here.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { clearSearchIndex } from '$lib/settings/settings-search'
import NavigationAndFileOpsSection from './NavigationAndFileOpsSection.svelte'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'fileOperations.allowFileExtensionChanges') return 'ask'
    if (key === 'behavior.doubleClickPaneNavigatesToParent') return true
    if (key === 'behavior.textEditorApp') return 'system'
    if (key === 'behavior.openTerminalHereApp') return 'com.apple.Terminal'
    if (key === 'operationLog.maxAge') return 0
    if (key === 'operationLog.maxSize') return 3221225472
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

// `isMacOS()` reads false under jsdom on every host, so each test says which platform it's on.
const isMacOS = vi.hoisted(() => vi.fn(() => true))
vi.mock('$lib/shortcuts/key-capture', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/shortcuts/key-capture')>()),
  isMacOS: () => isMacOS(),
}))

// The Text editor and Terminal rows ask the backend which apps are on this Mac on
// mount. The Show in Finder card asks macOS too; `null` keeps it hidden here.
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  listTextEditors: vi.fn(() =>
    Promise.resolve({
      data: { defaultAppName: 'TextEdit', defaultAppIcon: null, apps: [], chosenId: 'system' },
      timedOut: false,
    }),
  ),
  listTerminalApps: vi.fn(() =>
    Promise.resolve({
      data: {
        apps: [{ id: 'com.apple.Terminal', displayName: 'Terminal', icon: null, isRunning: false }],
        chosenId: 'com.apple.Terminal',
      },
      timedOut: false,
    }),
  ),
  getRevealHandlerState: vi.fn(() => Promise.resolve(null)),
}))

async function mountSection(searchQuery = ''): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(NavigationAndFileOpsSection, { target, props: { searchQuery } })
  await tick()
  return target
}

function cardLabels(target: HTMLElement): string[] {
  return Array.from(target.querySelectorAll('.section-card-label')).map((el) => el.textContent.trim())
}

function labelFors(target: HTMLElement): (string | null)[] {
  return Array.from(target.querySelectorAll('label.setting-label')).map((el) => el.getAttribute('for'))
}

beforeEach(() => {
  isMacOS.mockReturnValue(true)
  // The index drops macOS-only settings off macOS, and it memoizes.
  clearSearchIndex()
})

describe('NavigationAndFileOpsSection', () => {
  it('renders Navigation, File operations, Text editor, Terminal, and Operation log cards in that order', async () => {
    const target = await mountSection()
    expect(cardLabels(target)).toEqual(['Navigation', 'File operations', 'Text editor', 'Terminal', 'Operation log'])
    target.remove()
  })

  it('renders neither the Text editor nor the Terminal card off macOS', async () => {
    isMacOS.mockReturnValue(false)
    const target = await mountSection()
    expect(cardLabels(target)).toEqual(['Navigation', 'File operations', 'Operation log'])
    const fors = labelFors(target)
    expect(fors).not.toContain('behavior.textEditorApp')
    expect(fors).not.toContain('behavior.openTerminalHereApp')
    target.remove()
  })

  it('puts each setting in its card', async () => {
    const target = await mountSection()
    const fors = labelFors(target)
    expect(fors).toContain('behavior.doubleClickPaneNavigatesToParent')
    expect(fors).toContain('fileOperations.allowFileExtensionChanges')
    expect(fors).toContain('behavior.textEditorApp')
    expect(fors).toContain('behavior.openTerminalHereApp')
    expect(fors).toContain('operationLog.maxAge')
    expect(fors).toContain('operationLog.maxSize')
    target.remove()
  })

  it('surfaces the Text editor card under a search for an editor', async () => {
    const target = await mountSection('Sublime Text')
    expect(cardLabels(target)).toEqual(['Text editor'])
    target.remove()
  })

  it('surfaces the Terminal card under a search for a terminal app', async () => {
    const target = await mountSection('Ghostty')
    expect(cardLabels(target)).toEqual(['Terminal'])
    target.remove()
  })

  it('does not render the former Advanced mirror rows', async () => {
    const target = await mountSection()
    const fors = labelFors(target)
    expect(fors).not.toContain('fileOperations.maxConflictsToShow')
    expect(fors).not.toContain('fileOperations.progressUpdateInterval')
    target.remove()
  })

  it('hides every card when the search matches nothing on this page', async () => {
    const target = await mountSection('zzznomatch')
    expect(target.querySelectorAll('.section-card')).toHaveLength(0)
    target.remove()
  })

  it('shows only the matching card under a scoped search', async () => {
    const target = await mountSection('double-click')
    expect(cardLabels(target)).toEqual(['Navigation'])
    target.remove()
  })

  it('surfaces the Operation log card under a retention search', async () => {
    const target = await mountSection('retention')
    expect(cardLabels(target)).toEqual(['Operation log'])
    target.remove()
  })
})
