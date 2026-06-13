/**
 * Tier 3 a11y + behavior tests for `WhatsNewDialog.svelte`.
 *
 * The dialog renders the changelog slice from `whatsNewState`, a "See full changelog"
 * link, and a footer with the opt-out + Close actions. IPC and the toast are mocked so
 * the tests run deterministically.
 */

import { describe, it, vi, expect, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import WhatsNewDialog from './WhatsNewDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { whatsNewState, closeWhatsNew } from './whats-new-trigger.svelte'

const openExternalUrlMock = vi.fn<(url: string) => Promise<void>>(() => Promise.resolve())
vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  openExternalUrl: (url: string) => openExternalUrlMock(url),
}))

const setSettingMock = vi.fn()
vi.mock('$lib/settings', () => ({
  setSetting: (id: string, value: unknown) => {
    setSettingMock(id, value)
  },
}))

const addToastMock = vi.fn<(...args: unknown[]) => void>()
vi.mock('$lib/ui/toast', () => ({
  addToast: (...args: unknown[]) => {
    addToastMock(...args)
  },
}))

const sampleReleases = [
  {
    version: '0.26.0',
    date: '2026-06-11',
    lead: 'A focused release with a couple of nice touches.',
    sections: [
      { title: 'Added', entries: ['A shiny **new** thing', 'Inline `code` survives'] },
      { title: 'Fixed', entries: ['Squashed a flicker'] },
    ],
  },
  {
    version: '0.25.0',
    date: '2026-06-01',
    lead: null,
    sections: [{ title: 'Changed', entries: ['Tweaked a default'] }],
  },
]

function setReleases(releases: typeof sampleReleases | [], allowEmpty: boolean): void {
  whatsNewState.releases = releases
  whatsNewState.allowEmpty = allowEmpty
  whatsNewState.open = true
}

async function mountDialog(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(WhatsNewDialog, { target, props: {} })
  await tick()
  return target
}

function findButton(target: HTMLElement, text: string): HTMLButtonElement | undefined {
  return Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === text)
}

describe('WhatsNewDialog', () => {
  beforeEach(() => {
    closeWhatsNew()
    openExternalUrlMock.mockClear()
    setSettingMock.mockClear()
    addToastMock.mockClear()
  })

  it('default render has no a11y violations', async () => {
    setReleases(sampleReleases, false)
    const target = await mountDialog()
    await expectNoA11yViolations(target)
  })

  it('empty-state render has no a11y violations', async () => {
    setReleases([], true)
    const target = await mountDialog()
    await expectNoA11yViolations(target)
  })

  it('renders each release version, date, section title, and entry', async () => {
    setReleases(sampleReleases, false)
    const target = await mountDialog()

    expect(target.textContent).toContain('0.26.0')
    expect(target.textContent).toContain('2026-06-11')
    expect(target.textContent).toContain('Added')
    expect(target.textContent).toContain('Fixed')
    expect(target.textContent).toContain('Changed')
    expect(target.textContent).toContain('Squashed a flicker')
    // Inline markdown is rendered, not shown raw.
    expect(target.querySelector('.entries li strong')?.textContent).toBe('new')
    expect(target.querySelector('.entries li code')?.textContent).toBe('code')
  })

  it('shows the empty state with the changelog link when there are no releases', async () => {
    setReleases([], true)
    const target = await mountDialog()
    expect(target.querySelector('.empty')).not.toBeNull()
    expect(Array.from(target.querySelectorAll('a')).some((a) => a.textContent.includes('See full changelog'))).toBe(
      true,
    )
  })

  it('routes the "See full changelog" link through the opener instead of navigating', async () => {
    setReleases(sampleReleases, false)
    const target = await mountDialog()
    const link = Array.from(target.querySelectorAll('a')).find((a) => a.textContent.includes('See full changelog'))
    link?.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
    await tick()
    expect(openExternalUrlMock).toHaveBeenCalledWith('https://getcmdr.com/changelog/')
  })

  it('Close button closes the dialog', async () => {
    setReleases(sampleReleases, false)
    const target = await mountDialog()
    findButton(target, 'Close')?.click()
    await tick()
    expect(whatsNewState.open).toBe(false)
  })

  it('opt-out flips the setting, closes, and fires a toast', async () => {
    setReleases(sampleReleases, false)
    const target = await mountDialog()
    findButton(target, 'Not interested in changelogs')?.click()
    await tick()

    expect(setSettingMock).toHaveBeenCalledWith('whatsNew.showOnUpdate', false)
    expect(whatsNewState.open).toBe(false)
    expect(addToastMock).toHaveBeenCalled()
  })
})
