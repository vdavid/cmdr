/**
 * Tier-3 tests for `NetworkSection.svelte` (File systems › SMB/Network shares).
 *
 * Pins three things:
 *   1. The page renders two cards: Connection and Performance and timeouts.
 *   2. `network.smbConcurrency` does NOT render here: it lives only in Advanced
 *      now (its single home), so the Performance card holds share cache and
 *      timeout rows but not concurrency.
 *   3. Card grouping under search: a search matching only a Connection-card term
 *      leaves NO empty "Performance and timeouts" frame standing (card
 *      visibility is section-owned via `anyVisible`).
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import NetworkSection from './NetworkSection.svelte'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'network.enabled') return true
    if (key === 'network.directSmbConnection') return true
    if (key === 'network.shareCacheDuration') return 30000
    if (key === 'network.timeoutMode') return 'normal'
    if (key === 'network.customTimeout') return 15
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/tauri-commands', () => ({
  invoke: vi.fn(() => Promise.resolve()),
  openSystemSettingsUrl: vi.fn(() => Promise.resolve()),
}))

async function mountSection(searchQuery = ''): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(NetworkSection, { target, props: { searchQuery } })
  await tick()
  return target
}

function cardLabels(target: HTMLElement): string[] {
  return Array.from(target.querySelectorAll('.section-card-label')).map((el) => el.textContent.trim())
}

describe('NetworkSection card groups', () => {
  it('renders both cards with no search', async () => {
    const target = await mountSection()
    expect(cardLabels(target)).toEqual(expect.arrayContaining(['Connection', 'Performance and timeouts']))
    target.remove()
  })

  it('does not render the smbConcurrency row (it lives only in Advanced now)', async () => {
    const target = await mountSection()
    // `SettingRow` renders `<label for={id}>`, so each row's `for` identifies its setting.
    const labelFors = Array.from(target.querySelectorAll('label.setting-label')).map((el) => el.getAttribute('for'))
    expect(labelFors).not.toContain('network.smbConcurrency')
    // The Performance card still holds its own rows.
    expect(labelFors).toContain('network.shareCacheDuration')
    target.remove()
  })

  it('shows only the Connection card when searching a discovery term, leaving no empty cards', async () => {
    const target = await mountSection('discovery')
    const labels = cardLabels(target)
    expect(labels).toContain('Connection')
    expect(labels).not.toContain('Performance and timeouts')
    expect(target.querySelectorAll('.section-card')).toHaveLength(1)
    target.remove()
  })
})
