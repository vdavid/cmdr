/**
 * Tier-3 tests for `NavigationAndFileOpsSection.svelte`
 * (Behavior › Navigation & file ops).
 *
 * Three labeled cards: "Navigation" (the double-click-to-parent switch), "File
 * operations" (the file-extension-change radio), and "Operation log" (the
 * retention limits). The conflict/progress settings live in Advanced (their
 * single home), never mirrored here.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import NavigationAndFileOpsSection from './NavigationAndFileOpsSection.svelte'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'fileOperations.allowFileExtensionChanges') return 'ask'
    if (key === 'behavior.doubleClickPaneNavigatesToParent') return true
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

describe('NavigationAndFileOpsSection', () => {
  it('renders Navigation, File operations, and Operation log cards in that order', async () => {
    const target = await mountSection()
    expect(target.querySelectorAll('.section-card')).toHaveLength(3)
    expect(cardLabels(target)).toEqual(['Navigation', 'File operations', 'Operation log'])
    target.remove()
  })

  it('puts each setting in its card', async () => {
    const target = await mountSection()
    const fors = labelFors(target)
    expect(fors).toContain('behavior.doubleClickPaneNavigatesToParent')
    expect(fors).toContain('fileOperations.allowFileExtensionChanges')
    expect(fors).toContain('operationLog.maxAge')
    expect(fors).toContain('operationLog.maxSize')
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
