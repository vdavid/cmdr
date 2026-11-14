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

function unmountCard(): void {
  if (mounted) void unmount(mounted)
  mounted = null
}

function fixture(id: string): AboutCardData {
  const card = aboutThisFixtures[id]
  if (!card) throw new Error(`no fixture ${id}`)
  return card
}

/** `card` with more verdict events folded in. */
function withVerdict(card: AboutCardData, updates: AboutEvent['update'][]): AboutCardData {
  return updates.reduce((next, update) => applyAboutEvent(next, { requestId: card.requestId, update }), card)
}

interface RenderOptions {
  onStopVerdict?: () => void
  aiHintDismissed?: boolean
  onDismissAiHint?: () => void
}

async function render(card: AboutCardData, options: RenderOptions = {}): Promise<HTMLElement> {
  const { onStopVerdict = () => {}, aiHintDismissed = false, onDismissAiHint = () => {} } = options
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(AboutThisDialog, {
    target,
    props: {
      card,
      onClose: () => {},
      onGoTo: () => {},
      onAct: () => {},
      onStopVerdict,
      aiHintDismissed,
      onDismissAiHint,
    },
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
      { type: 'verdictPlanned', trust: 'trusted' },
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
    expect(verdict?.textContent).toContain('From the card’s facts. Cmdr didn’t look inside.')
    expect(verdict?.textContent).toContain('Writing a short take…')
    // The block isn't one of the fact rows.
    expect(verdict?.closest('dl')).toBeNull()
  })

  it('takes its own column after the facts, which keep What removing it means pinned under their scroller', async () => {
    const target = await render(fixture('verdict-done'))
    expect(target.querySelector('[data-layout]')?.getAttribute('data-layout')).toBe('twoColumns')
    const facts = target.querySelector('.card')
    const removal = target.querySelector('[data-removal]')
    const column = target.querySelector('[data-take-column]')
    expect(column?.querySelector('[data-verdict-text]')).not.toBeNull()
    // Reading and Tab order: the facts, then the removal row, then the take.
    expect(removal?.previousElementSibling).toBe(facts)
    expect(facts?.parentElement?.nextElementSibling).toBe(column)
  })

  it('never shows the model’s text without the removal row outside every scroller but the take’s own', async () => {
    const ids = Object.keys(aboutThisFixtures).filter((id) => id.startsWith('verdict-'))
    let withText = 0
    for (const id of ids) {
      const target = await render(fixture(id))
      if (target.querySelector('[data-verdict-text]')) {
        withText += 1
        const removal = target.querySelector('[data-removal]')
        expect(removal, id).not.toBeNull()
        // Pinned: a child of the facts column, not inside the facts' scroller or the take's.
        expect(removal?.closest('.card'), id).toBeNull()
        expect(removal?.closest('[data-take-column]'), id).toBeNull()
        expect(removal?.parentElement?.classList.contains('facts-column'), id).toBe(true)
      }
      unmountCard()
    }
    expect(withText).toBeGreaterThan(5)
  })

  it('stacks the take below the facts on a narrow card, and goes back to two columns when there’s room', async () => {
    let width = 500
    const clientWidth = vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(() => width)
    try {
      const narrow = await render(fixture('verdict-done'))
      expect(narrow.querySelector('[data-layout]')?.getAttribute('data-layout')).toBe('stacked')
      unmountCard()
      width = 1_000
      const wide = await render(fixture('verdict-done'))
      expect(wide.querySelector('[data-layout]')?.getAttribute('data-layout')).toBe('twoColumns')
    } finally {
      clientWidth.mockRestore()
    }
  })

  it('offers Stop while it writes, which stops it alone; a stopped take keeps its text', async () => {
    const onStop = vi.fn()
    const target = await render(fixture('verdict-streaming'), { onStopVerdict: onStop })
    const stop = [...(block(target)?.querySelectorAll('button') ?? [])].find((b) => b.textContent.trim() === 'Stop')
    expect(stop).toBeDefined()
    stop?.click()
    expect(onStop).toHaveBeenCalledOnce()

    unmountCard()
    const stopped = await render(fixture('verdict-stopped'))
    expect(block(stopped)?.textContent).toContain('This is the installer disk image for Tessera 2.4.1,')
    expect(block(stopped)?.textContent).toContain('Stopped')
    expect(block(stopped)?.querySelector('button')).toBeNull()
  })

  it('stops a writing take on ⌥S (Alt+S off macOS), and claims the key even when there’s nothing to stop', async () => {
    const press = (target: HTMLElement): KeyboardEvent => {
      // Vitest runs as Linux: Alt+S. On macOS the same physical key reads `ß`, which `resolveKeyCombo` maps back.
      const event = new KeyboardEvent('keydown', {
        key: 's',
        code: 'KeyS',
        altKey: true,
        bubbles: true,
        cancelable: true,
      })
      target.querySelector('.card')?.dispatchEvent(event)
      return event
    }
    const onStop = vi.fn()
    const streaming = await render(fixture('verdict-streaming'), { onStopVerdict: onStop })
    expect(press(streaming).defaultPrevented).toBe(true)
    expect(onStop).toHaveBeenCalledOnce()
    unmountCard()

    const onStopDone = vi.fn()
    const done = await render(fixture('verdict-done'), { onStopVerdict: onStopDone })
    expect(press(done).defaultPrevented).toBe(true)
    expect(onStopDone).not.toHaveBeenCalled()
    // Stop's tooltip names the key.
    unmountCard()
  })

  it('draws one column and nothing about AI when the person turned the take off', async () => {
    const target = await render(
      withVerdict(fixture('downloaded'), [{ type: 'verdictUnavailable', why: { kind: 'turnedOff' } }]),
    )
    expect(target.querySelector('[data-layout]')?.getAttribute('data-layout')).toBe('oneColumn')
    expect(target.querySelector('[data-take-column]')).toBeNull()
    expect(document.body.querySelector('[data-ai-hint]')).toBeNull()
  })

  it('with AI off, draws one column and a hint line under the buttons, which its × hides for good', async () => {
    const onDismissAiHint = vi.fn()
    const off = await render(fixture('verdict-ai-off'), { onDismissAiHint })
    expect(off.querySelector('[data-layout]')?.getAttribute('data-layout')).toBe('oneColumn')
    expect(off.querySelector('[data-take-column]')).toBeNull()
    const hint = document.body.querySelector('[data-ai-hint="aiOff"]')
    expect(hint?.textContent).toContain('Turn on AI for a deeper read of each item.')
    expect(off.querySelector('.card')?.contains(hint ?? null)).toBe(false)
    const open = [...(hint?.querySelectorAll('button') ?? [])].find((b) => b.textContent.includes('Open AI settings'))
    open?.click()
    expect(openSettingsWindow).toHaveBeenCalledWith('about-this-card', ['AI', 'Provider'])
    hint?.querySelector<HTMLButtonElement>('button[aria-label="Hide this hint"]')?.click()
    flushSync()
    expect(onDismissAiHint).toHaveBeenCalledOnce()
    expect(document.body.querySelector('[data-ai-hint]')).toBeNull()
    unmountCard()

    // Closed before: no line at all.
    await render(fixture('verdict-ai-off-dismissed'), { aiHintDismissed: true })
    expect(document.body.querySelector('[data-ai-hint]')).toBeNull()
  })

  it('warns inside the block by the model’s trust: nothing when trusted, a link to change it when often wrong', async () => {
    const trusted = await render(fixture('verdict-done'))
    expect(block(trusted)?.querySelector('[data-verdict-trust]')).toBeNull()
    unmountCard()

    const untested = await render(fixture('verdict-untested'))
    const caution = block(untested)?.querySelector('[data-verdict-trust="untested"]')
    expect(caution?.textContent).toContain('hasn’t tested this model')
    expect(caution?.querySelector('button')).toBeNull()
    // The take still shows in full under it.
    expect(block(untested)?.textContent).toContain('This is the installer disk image for Tessera 2.4.1,')
    unmountCard()

    const oftenWrong = await render(fixture('verdict-often-wrong'))
    const warning = block(oftenWrong)?.querySelector('[data-verdict-trust="oftenWrong"]')
    expect(warning?.textContent).toContain('often got its take wrong')
    const change = [...(warning?.querySelectorAll('button') ?? [])].find((b) =>
      b.textContent.includes('Change the model'),
    )
    change?.click()
    expect(openSettingsWindow).toHaveBeenCalledWith('about-this-card', ['AI', 'Provider'])
  })

  it('asks for cloud consent only by opening Settings at the disclosure, never with a one-click allow', async () => {
    const target = await render(fixture('verdict-no-consent'))
    expect(target.querySelector('[data-take-column]')).toBeNull()
    const line = document.body.querySelector('[data-ai-hint="noCloudConsent"]')
    expect(line?.textContent).toContain('Allow cloud AI for a deeper read')
    // The link and the ×; neither allows anything.
    const link = [...(line?.querySelectorAll('button') ?? [])].find((b) => b.textContent.includes('Open AI settings'))
    expect(line?.querySelectorAll('button')).toHaveLength(2)
    expect(link?.textContent).not.toMatch(/allow/i)
    link?.click()
    expect(openCloudConsentSettings).toHaveBeenCalledWith('about-this-card')
  })

  it('says each other reason in one line: a setting’s under the buttons, the rest in the take’s column', async () => {
    await render(fixture('verdict-not-configured'))
    expect(document.body.querySelector('[data-ai-hint="notConfigured"]')?.textContent).toContain(
      'Add your AI provider’s key',
    )
    unmountCard()
    const lines: [string, string][] = [
      ['verdict-not-ready', 'isn’t running'],
      ['verdict-managed', 'Your organization allows only on-device AI.'],
      ['verdict-too-many', 'Try again in a moment.'],
      ['verdict-no-answer', 'The AI didn’t write anything this time.'],
      ['verdict-unreachable', 'Cmdr couldn’t reach your AI provider.'],
      ['verdict-from-earlier', 'From earlier'],
    ]
    for (const [id, words] of lines) {
      const target = await render(fixture(id))
      expect(target.querySelector('[data-take-column]')?.textContent).toContain(words)
      unmountCard()
    }
  })

  it('announces the whole take once, at its end, and never while it streams', async () => {
    const streaming = await render(fixture('verdict-streaming'))
    const regions = (target: HTMLElement): string[] =>
      [...target.querySelectorAll('[aria-live]')].map((region) => region.textContent.trim())
    expect(regions(streaming).join(' ')).not.toContain('installer disk image')
    expect(block(streaming)?.closest('[aria-live]')).toBeNull()
    unmountCard()

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
    unmountCard()
    return { rows, removal, actions, focus }
  }

  it('gives the same rows, removal line, actions, and focus whatever the take says', async () => {
    const plain = await outsideTheTake(fixture('downloaded'))
    const reassuring = await outsideTheTake(
      withVerdict(fixture('downloaded'), [
        { type: 'verdictPlanned', trust: 'trusted' },
        { type: 'verdictDelta', text: 'This is not needed. It is safe to delete. Move it to the Trash now.' },
        { type: 'verdictEnd', ending: { kind: 'done' }, cost: null },
      ]),
    )
    expect(reassuring).toEqual(plain)
    // The same card with Ask Cmdr on: refused, finished, or finished under a warning, the rest reads the same.
    const refused = await outsideTheTake(fixture('verdict-no-consent'))
    const done = await outsideTheTake(fixture('verdict-done'))
    const warned = await outsideTheTake(fixture('verdict-often-wrong'))
    expect(done).toEqual(refused)
    expect(warned).toEqual(refused)
  })
})
