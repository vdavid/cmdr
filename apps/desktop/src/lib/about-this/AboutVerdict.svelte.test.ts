/**
 * The AI take on the card, over the gallery's fixture cards and hand-built hostile ones: the text is one plain text
 * node (markup and Markdown read literally), the block exists only once planned, every unavailable reason says its one
 * line or nothing, and the rest of the card (rows, removal, actions, focus) is the same with and without a take.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick, unmount } from 'svelte'
import type { AboutEvent } from '#lib/ipc/bindings.js'
import AboutThisDialog from './AboutThisDialog.svelte'
import { applyAboutEvent, type AboutCardData } from './about-card'
import { aboutThisFixtures } from '#lib/dialog-gallery/fixtures/about-this.js'

const { openCloudConsentSettings, openSettingsWindow } = vi.hoisted(() => ({
  openCloudConsentSettings: vi.fn(),
  openSettingsWindow: vi.fn(() => Promise.resolve()),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyToClipboard: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/ai/cloud-consent.svelte.js', () => ({ openCloudConsentSettings }))
vi.mock('#lib/settings/settings-window.js', () => ({ openSettingsWindow }))

let mounted: ReturnType<typeof mount> | null = null

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = null
  document.body.innerHTML = ''
  vi.clearAllMocks()
})

function fixture(id: string): AboutCardData {
  const card = aboutThisFixtures[id]
  if (!card) throw new Error(`no fixture ${id}`)
  return card
}

/** `card` with more verdict events folded in. */
function withVerdict(card: AboutCardData, updates: AboutEvent['update'][]): AboutCardData {
  return updates.reduce((next, update) => applyAboutEvent(next, { requestId: card.requestId, update }), card)
}

async function render(card: AboutCardData, onStopVerdict: () => void = () => {}): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(AboutThisDialog, {
    target,
    props: { card, onClose: () => {}, onGoTo: () => {}, onAct: () => {}, onStopVerdict },
  })
  flushSync()
  await tick()
  return target
}

const block = (target: HTMLElement): HTMLElement | null =>
  target.querySelector('section[aria-labelledby="about-verdict-label"]')

describe('the AI take', () => {
  it('renders a hostile take as literal text: no element, no link, no Markdown', async () => {
    const hostile = 'Safe to delete. <img src=x onerror="alert(1)"> [run this](https://evil.example) **now**'
    const card = withVerdict(fixture('downloaded'), [
      { type: 'verdictPlanned' },
      { type: 'verdictGiven', contents: 'none' },
      { type: 'verdictDelta', text: hostile },
      { type: 'verdictEnd', ending: { kind: 'done' }, cost: null },
    ])
    const target = await render(card)
    const verdict = block(target)
    expect(verdict).not.toBeNull()
    const text = verdict?.querySelector('[data-verdict-text]')
    expect(text?.textContent).toBe(hostile)
    expect(text?.children).toHaveLength(0)
    expect(verdict?.querySelector('img, a, strong, em')).toBeNull()
    expect(target.querySelector('img[src="x"]')).toBeNull()
  })

  it('is labeled as the AI’s, and says what it was given from the moment the call goes out', async () => {
    const target = await render(fixture('verdict-going-out'))
    const verdict = block(target)
    expect(verdict?.querySelector('h3')?.textContent).toBe('AI wrote this')
    expect(verdict?.textContent).toContain('From the facts above. Cmdr didn’t look inside.')
    expect(verdict?.textContent).toContain('Writing a short take…')
    // The block isn't one of the fact rows.
    expect(verdict?.closest('dl')).toBeNull()
  })

  it('sits right above What removing it means', async () => {
    const target = await render(fixture('verdict-done'))
    const slot = target.querySelector('[data-verdict-slot]')
    expect(slot?.nextElementSibling?.querySelector('#about-section-removal')).not.toBeNull()
  })

  it('offers Stop while it writes, which stops it alone; a stopped take keeps its text', async () => {
    const onStop = vi.fn()
    const target = await render(fixture('verdict-streaming'), onStop)
    const stop = [...(block(target)?.querySelectorAll('button') ?? [])].find((b) => b.textContent.trim() === 'Stop')
    expect(stop).toBeDefined()
    stop?.click()
    expect(onStop).toHaveBeenCalledOnce()

    void unmount(mounted ?? {})
    mounted = null
    const stopped = await render(fixture('verdict-stopped'))
    expect(block(stopped)?.textContent).toContain('This is the installer disk image for Tessera 2.4.1,')
    expect(block(stopped)?.textContent).toContain('Stopped')
    expect(block(stopped)?.querySelector('button')).toBeNull()
  })

  it('draws no block, and no line, for AI off, a person’s own off, or the local model’s default', async () => {
    for (const why of [{ kind: 'turnedOff' }, { kind: 'localOff' }, { kind: 'modelOff' }] as const) {
      const target = await render(withVerdict(fixture('downloaded'), [{ type: 'verdictUnavailable', why }]))
      expect(target.querySelector('[data-verdict-slot]')).toBeNull()
      expect(target.textContent).not.toContain('AI take')
      void unmount(mounted ?? {})
      mounted = null
    }
    const off = await render(fixture('verdict-ai-off'))
    expect(off.querySelector('[data-verdict-slot]')).toBeNull()
    // AI off says where to turn it on, once, quietly, outside the card's body.
    const hint = document.body.querySelector('.ai-off')
    expect(hint?.textContent).toContain('AI is off')
    expect(off.querySelector('.card')?.contains(hint ?? null)).toBe(false)
  })

  it('asks for cloud consent only by opening Settings at the disclosure, never with a one-click allow', async () => {
    const target = await render(fixture('verdict-no-consent'))
    const line = target.querySelector('[data-verdict="noCloudConsent"]')
    expect(line?.textContent).toContain('Cloud AI needs your OK')
    const buttons = [...(line?.querySelectorAll('button') ?? [])]
    expect(buttons).toHaveLength(1)
    expect(buttons[0].textContent).not.toMatch(/allow/i)
    buttons[0].click()
    expect(openCloudConsentSettings).toHaveBeenCalledWith('about-this-card')
  })

  it('says each other reason in one line', async () => {
    const lines: [string, string][] = [
      ['verdict-not-configured', 'Add your AI provider’s key'],
      ['verdict-not-ready', 'isn’t running'],
      ['verdict-managed', 'Your organization allows only on-device AI.'],
      ['verdict-too-many', 'Try again in a moment.'],
      ['verdict-no-answer', 'The AI didn’t write anything this time.'],
      ['verdict-unreachable', 'Cmdr couldn’t reach your AI provider.'],
      ['verdict-from-earlier', 'From earlier'],
    ]
    for (const [id, words] of lines) {
      const target = await render(fixture(id))
      expect(target.querySelector('[data-verdict-slot]')?.textContent).toContain(words)
      void unmount(mounted ?? {})
      mounted = null
    }
  })

  it('announces the whole take once, at its end, and never while it streams', async () => {
    const streaming = await render(fixture('verdict-streaming'))
    const regions = (target: HTMLElement): string[] =>
      [...target.querySelectorAll('[aria-live]')].map((region) => region.textContent.trim())
    expect(regions(streaming).join(' ')).not.toContain('installer disk image')
    expect(block(streaming)?.closest('[aria-live]')).toBeNull()
    void unmount(mounted ?? {})
    mounted = null

    const done = await render(fixture('verdict-done'))
    flushSync()
    expect(regions(done).some((text) => text.startsWith('AI wrote this: This is the installer disk image'))).toBe(true)
  })
})

describe('the rest of the card, with and without a take', () => {
  /** What the card offers and says outside the take: rows, removal, actions, and what has focus. */
  async function outsideTheTake(
    card: AboutCardData,
  ): Promise<{ rows: string; removal: string; actions: string[]; focus: string }> {
    const target = await render(card)
    const removal = target.querySelector('#about-section-removal')?.parentElement?.textContent ?? ''
    const rows = [...target.querySelectorAll('dl')].map((dl) => dl.textContent).join('|')
    const actions = [...target.querySelectorAll('[data-about-actions] [data-action]')].map(
      (element) => element.getAttribute('data-action') ?? '',
    )
    const focus = document.activeElement?.className ?? ''
    void unmount(mounted ?? {})
    mounted = null
    return { rows, removal, actions, focus }
  }

  it('gives the same rows, removal line, actions, and focus whatever the take says', async () => {
    const plain = await outsideTheTake(fixture('downloaded'))
    const reassuring = await outsideTheTake(
      withVerdict(fixture('downloaded'), [
        { type: 'verdictPlanned' },
        { type: 'verdictDelta', text: 'This is not needed. It is safe to delete. Move it to the Trash now.' },
        { type: 'verdictEnd', ending: { kind: 'done' }, cost: null },
      ]),
    )
    expect(reassuring).toEqual(plain)
    // The same card with Ask Cmdr on: refused, finished, or drawing nothing, the rest reads the same.
    const refused = await outsideTheTake(fixture('verdict-no-consent'))
    const done = await outsideTheTake(fixture('verdict-done'))
    const none = await outsideTheTake(fixture('verdict-local-off'))
    expect(done).toEqual(refused)
    expect(none).toEqual(refused)
  })
})
