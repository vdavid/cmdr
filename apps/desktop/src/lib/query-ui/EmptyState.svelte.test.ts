import { beforeEach, describe, it, expect, vi } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { mount, tick } from 'svelte'

vi.mock('#lib/ai/cloud-consent.svelte.js', () => ({ openCloudConsentSettings: vi.fn() }))

import EmptyState from './EmptyState.svelte'
import { openCloudConsentSettings } from '#lib/ai/cloud-consent.svelte.js'

// The chips are spoken in macOS's glyphs here (⌥F, ⌘Y, ⏎); the off-macOS spelling has its own cases.
beforeEach(() => withPlatform('macos'))

describe('EmptyState', () => {
  it('shows three AI prompts when AI is enabled', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, {
      target,
      props: { aiEnabled: true, indexEntryCount: 100, onPick: () => {} },
    })
    await tick()
    const chips = target.querySelectorAll('.example-chip')
    expect(chips).toHaveLength(3)
    const labels = Array.from(chips).map((c) => c.textContent.trim())
    expect(labels.some((l) => l.includes('large files modified this week'))).toBe(true)
    expect(labels.some((l) => l.includes('screenshots'))).toBe(true)
    expect(labels.some((l) => l.includes('PDFs from the last 7 days'))).toBe(true)
    target.remove()
  })

  it('shows filename patterns when AI is disabled', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, {
      target,
      props: { aiEnabled: false, indexEntryCount: 100, onPick: () => {} },
    })
    await tick()
    const labels = Array.from(target.querySelectorAll('.example-chip')).map((c) => c.textContent.trim())
    expect(labels.some((l) => l.includes('*.pdf'))).toBe(true)
    expect(labels.some((l) => l.includes('*.dmg'))).toBe(true)
    expect(labels.some((l) => l.includes('screenshot*'))).toBe(true)
    target.remove()
  })

  it('formats the entry count with locale thousands separators', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, {
      target,
      props: { aiEnabled: false, indexEntryCount: 10_100_000, onPick: () => {} },
    })
    await tick()
    const status = target.querySelector('.index-status')?.textContent ?? ''
    expect(status).toContain('10,100,000')
    target.remove()
  })

  it.each([false, true])(
    'shows the in-dialog keyboard tip line, AI on or off (%s): ⌘N and ⌘Y, no ⌘Enter',
    async (aiEnabled) => {
      const target = document.createElement('div')
      document.body.appendChild(target)
      mount(EmptyState, {
        target,
        props: { aiEnabled, indexEntryCount: 1, onPick: () => {} },
      })
      await tick()
      const tip = target.querySelector('.tip')?.textContent ?? ''
      expect(tip).toContain('⌘N')
      expect(tip).toContain('⌘Y')
      // ⌘H is Hide on macOS, and ⌘Enter does nothing in the dialog (Enter runs the search).
      expect(tip).not.toContain('⌘H')
      expect(tip).not.toContain('⌘↩')
      expect(tip).not.toContain('⌘Enter')
      // ⌘F opens the dialog from the explorer; once the dialog is open the
      // shortcut is moot, so we explicitly do NOT advertise it inside the
      // empty state.
      expect(tip).not.toContain('⌘F')
      target.remove()
    },
  )

  it('renders consumer-provided examples when passed', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, {
      target,
      props: {
        aiEnabled: true,
        indexEntryCount: 0,
        examples: [
          { label: 'all image files', mode: 'ai', query: 'all image files' },
          { label: 'logs newer than a week', mode: 'ai', query: 'logs newer than a week' },
          { label: 'files bigger than 5 MB', mode: 'ai', query: 'files bigger than 5 MB' },
        ],
        onPick: () => {},
      },
    })
    await tick()
    const labels = Array.from(target.querySelectorAll('.example-chip')).map((c) => c.textContent.trim())
    expect(labels.some((l) => l.includes('all image files'))).toBe(true)
    expect(labels.some((l) => l.includes('logs newer than a week'))).toBe(true)
    expect(labels.some((l) => l.includes('files bigger than 5 MB'))).toBe(true)
    // None of the Search-flavoured defaults should show.
    expect(labels.some((l) => l.includes('PDFs from the last 7 days'))).toBe(false)
    target.remove()
  })

  it('hides the index-status line when indexEntryCount is 0 (Selection)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, {
      target,
      props: { aiEnabled: false, indexEntryCount: 0, onPick: () => {} },
    })
    await tick()
    expect(target.querySelector('.index-status')).toBeNull()
    target.remove()
  })

  it('calls onPick with the chosen chip', async () => {
    const onPick = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, {
      target,
      props: { aiEnabled: true, indexEntryCount: 1, onPick },
    })
    await tick()
    const firstChip = target.querySelector('.example-chip') as HTMLButtonElement
    firstChip.click()
    expect(onPick).toHaveBeenCalledTimes(1)
    const picked = onPick.mock.calls[0]?.[0] as { mode: string; query: string }
    expect(picked.mode).toBe('ai')
    expect(picked.query).toBe('large files modified this week')
    target.remove()
  })
})

describe.each(['windows', 'linux'] as const)('EmptyState tip on %s', (platform) => {
  beforeEach(() => withPlatform(platform))

  it('names the dialog’s own keys, with no Mac glyph', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, { target, props: { aiEnabled: false, indexEntryCount: 1, onPick: () => {} } })
    await tick()
    const tip = target.querySelector('.tip')?.textContent ?? ''
    expect(tip).toContain('Ctrl+N')
    expect(tip).toContain('Ctrl+H')
    expect(tip).not.toMatch(/[⌘⌥⌃⇧]/)
    target.remove()
  })
})

describe('EmptyState while cloud AI is off', () => {
  it('trades the AI prompts for the reason and a way into the switch', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, {
      target,
      props: { aiEnabled: true, aiBlocked: true, indexEntryCount: 100, onPick: () => {} },
    })
    await tick()
    expect(target.querySelector('.cloud-off')?.textContent).toContain(
      'Cloud AI is off. Allow it in Settings > AI to use this.',
    )
    const labels = Array.from(target.querySelectorAll('.example-chip')).map((c) => c.textContent.trim())
    // Filename patterns still work, so they stay; AI prompts would only be refused.
    expect(labels.some((l) => l.includes('large files modified this week'))).toBe(false)
    expect(labels.some((l) => l.includes('*.pdf'))).toBe(true)

    target.querySelector<HTMLButtonElement>('.cloud-off button')?.click()
    expect(openCloudConsentSettings).toHaveBeenCalledWith('query-ai-cloud-off')
    target.remove()
  })

  it("drops a consumer's AI examples but keeps its other ones", async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, {
      target,
      props: {
        aiEnabled: true,
        aiBlocked: true,
        indexEntryCount: 0,
        examples: [
          { label: 'all image files', mode: 'ai', query: 'all image files' },
          { label: '*.jpg', mode: 'filename', query: '*.jpg' },
        ],
        onPick: () => {},
      },
    })
    await tick()
    const labels = Array.from(target.querySelectorAll('.example-chip')).map((c) => c.textContent.trim())
    expect(labels).toEqual(['Aa *.jpg'])
    target.remove()
  })

  it('says nothing about cloud AI while it is allowed', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, {
      target,
      props: { aiEnabled: true, indexEntryCount: 100, onPick: () => {} },
    })
    await tick()
    expect(target.querySelector('.cloud-off')).toBeNull()
    target.remove()
  })
})
