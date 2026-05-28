import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import EmptyState from './EmptyState.svelte'

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

  it('shows the in-dialog keyboard tip line (AI off: ⌘N and ⌘H, no ⌘Enter)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, {
      target,
      props: { aiEnabled: false, indexEntryCount: 1, onPick: () => {} },
    })
    await tick()
    const tip = target.querySelector('.tip')?.textContent ?? ''
    expect(tip).toContain('⌘N')
    expect(tip).toContain('⌘H')
    // ⌘Enter is AI-gated and AI is off here.
    expect(tip).not.toContain('⌘Enter')
    // ⌘F opens the dialog from the explorer; once the dialog is open the
    // shortcut is moot, so we explicitly do NOT advertise it inside the
    // empty state.
    expect(tip).not.toContain('⌘F')
    target.remove()
  })

  it('adds the ⌘Enter AI hint when AI is enabled', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, {
      target,
      props: { aiEnabled: true, indexEntryCount: 1, onPick: () => {} },
    })
    await tick()
    const tip = target.querySelector('.tip')?.textContent ?? ''
    expect(tip).toContain('⌘N')
    expect(tip).toContain('⌘H')
    expect(tip).toContain('⌘Enter')
    target.remove()
  })

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
