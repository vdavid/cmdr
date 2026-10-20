/**
 * Tier 3 a11y tests for the Ask Cmdr rail and the pieces it renders: the two
 * gates (Ask Cmdr off, cloud AI off), the composer, a thread message, a tool line, an attachment chip, the
 * context gauge, and the cost footer.
 *
 * One file per component would cost about eight times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions.
 *
 * `askCmdrState` is one shared mutable object rather than four different ones:
 * each block resets the fields it reads in its own `beforeEach`, so no block can
 * inherit another's rail state.
 *
 * `AskCmdrSessions` and `BulkRenameReviewDialog` keep their own files: each
 * mocks explorer/viewer modules nothing else here touches.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync, mount, tick } from 'svelte'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import type { AttachmentRef, ConversationCost } from '#lib/tauri-commands/index.js'
import type { RailMessage, RailToolCall } from './ask-cmdr-trigger.svelte'
import type { ContextUsage } from './ask-cmdr-context-usage'

// `vi.hoisted` so the shared mutable state exists before the hoisted `vi.mock`
// factories run.
const { triggerState, flags, costMock, consentState, railSettings } = vi.hoisted(() => {
  // Plain mutable objects, so the gate blocks can move them before mounting. Annotations, not
  // `as`: the lint auto-fix strips an assertion it thinks is unnecessary.
  const consentState: { accepted: boolean | null; acceptedAt: number | null } = { accepted: true, acceptedAt: null }
  const railSettings: Record<string, unknown> = { 'askCmdr.enabled': true, 'ai.provider': 'off' }
  return {
    consentState,
    railSettings,
    triggerState: {
      streaming: false,
      width: 340,
      conversationId: null as number | null,
      messages: [] as unknown[],
      attachments: [] as unknown[],
    },
    flags: { overSoftCap: false },
    costMock: vi.fn<(id: number) => Promise<unknown>>(),
  }
})

// The union of what these blocks reach for. Each source file mocked a different
// slice of the trigger module; a component only calls its own, so an unused stub
// changes nothing for the others.
vi.mock('./ask-cmdr-trigger.svelte', () => ({
  askCmdrState: triggerState,
  isOverSoftCap: () => flags.overSoftCap,
  hasOlderMessages: () => false,
  loadOlderMessages: vi.fn(),
  closeRail: vi.fn(),
  openRail: vi.fn(() => Promise.resolve()),
  ensureThreadLoaded: vi.fn(() => Promise.resolve()),
  newChat: vi.fn(),
  setRailWidth: vi.fn(),
  sendMessage: vi.fn(),
  stopStreaming: vi.fn(),
  markRailFocused: vi.fn(),
  returnFocusToPane: vi.fn(),
  addAttachments: vi.fn(),
  removeAttachment: vi.fn(),
}))

// Ask Cmdr on, with no cloud involved, so the rail renders the chat unless a block moves it.
vi.mock('#lib/ai/cloud-consent.svelte.js', () => ({
  cloudConsentState: consentState,
  refreshCloudConsent: vi.fn(() => Promise.resolve()),
  openCloudConsentSettings: vi.fn(),
}))
vi.mock('#lib/settings/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getSetting: (id: string): unknown => railSettings[id],
  setSetting: vi.fn((id: string, value: unknown) => {
    railSettings[id] = value
  }),
  onSpecificSettingChange: () => () => {},
}))

vi.mock('./ask-cmdr-sessions.svelte', () => ({
  sessionsState: { open: false },
  openSessions: vi.fn(),
}))

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  askCmdrConversationCost: (id: number) => costMock(id),
}))

vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import AskCmdrAttachmentChip from './AskCmdrAttachmentChip.svelte'
import AskCmdrComposer from './AskCmdrComposer.svelte'
import AskCmdrGate from './AskCmdrGate.svelte'
import AskCmdrContextGauge from './AskCmdrContextGauge.svelte'
import AskCmdrCostFooter from './AskCmdrCostFooter.svelte'
import AskCmdrMessage from './AskCmdrMessage.svelte'
import AskCmdrRail from './AskCmdrRail.svelte'
import AskCmdrToolLine from './AskCmdrToolLine.svelte'
import { openCloudConsentSettings } from '#lib/ai/cloud-consent.svelte.js'
import { setSetting } from '#lib/settings/index.js'

/** A fresh container, appended to the document and ready to mount into. */
function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

/** The composer's "Ask about selection" read, with nothing to attach. */
const noSelection = () => Promise.resolve({ kind: 'refs' as const, refs: [], leftOut: 0 })

beforeEach(() => {
  triggerState.streaming = false
  triggerState.width = 340
  triggerState.conversationId = null
  triggerState.messages = []
  triggerState.attachments = []
  flags.overSoftCap = false
})

// Only the two blocks that format numbers pinned a locale; the rest ran on the
// harness default, so it's restored after every test.
afterEach(() => {
  _setLocaleForTests(null)
})

/**
 * Tier 3 a11y tests for `AskCmdrAttachmentChip.svelte`: a file/folder reference chip,
 * read-only under a sent message and removable in the composer. The remove button carries
 * an accessible label.
 */
describe('AskCmdrAttachmentChip a11y', () => {
  const fileRef: AttachmentRef = { path: '/Users/me/taxes.pdf', kind: 'file' }
  const folderRef: AttachmentRef = { path: '/Users/me/photos', kind: 'folder' }

  function mountChip(attachment: AttachmentRef, onRemove?: (path: string) => void): HTMLElement {
    const target = container()
    mount(AskCmdrAttachmentChip, { target, props: { attachment, onRemove } })
    return target
  }

  it('a read-only file chip has no a11y violations', async () => {
    const target = mountChip(fileRef)
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a removable folder chip has no a11y violations', async () => {
    const target = mountChip(folderRef, () => {})
    await tick()
    expect(target.querySelector('.chip-remove')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `AskCmdrComposer.svelte`.
 *
 * The message input plus its send/stop button. Covers the idle state (labeled input +
 * disabled send) and the streaming state (the button flips to Stop). The trigger store is
 * mocked to a plain object so the composer mounts without the full explorer-state chain.
 */
describe('AskCmdrComposer a11y', () => {
  function mountComposer(): HTMLElement {
    const target = container()
    mount(AskCmdrComposer, { target, props: { readSelection: noSelection } })
    return target
  }

  it('the idle composer has no a11y violations', async () => {
    const target = mountComposer()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('the streaming composer (stop button) has no a11y violations', async () => {
    triggerState.streaming = true
    const target = mountComposer()
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `AskCmdrGate.svelte`, the rail's two closed states, plus what each
 * button does. Neither grants cloud consent: the cloud gate only points at the switch.
 */
describe('AskCmdrGate a11y', () => {
  async function mountGate(kind: 'off' | 'cloudOff'): Promise<HTMLElement> {
    const target = container()
    mount(AskCmdrGate, { target, props: { kind } })
    await tick()
    return target
  }

  it('the Ask Cmdr off gate has no a11y violations', async () => {
    const target = await mountGate('off')
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('the cloud AI off gate has no a11y violations', async () => {
    const target = await mountGate('cloudOff')
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('turns Ask Cmdr on from the off gate', async () => {
    const target = await mountGate('off')
    target.querySelector<HTMLButtonElement>('.ask-cmdr-gate button')?.click()
    expect(setSetting).toHaveBeenCalledWith('askCmdr.enabled', true)
    target.remove()
  })

  it('sends the cloud gate to the Allow cloud AI switch, and grants nothing itself', async () => {
    const target = await mountGate('cloudOff')
    target.querySelector<HTMLButtonElement>('.ask-cmdr-gate button')?.click()
    expect(openCloudConsentSettings).toHaveBeenCalledWith('ask-cmdr-cloud-gate')
    target.remove()
  })
})

/**
 * Tier 3 a11y tests for `AskCmdrContextGauge.svelte`, the rail's context-usage gauge.
 *
 * The gauge is an ARIA meter: it must carry both a NAME and a value, or assistive tech
 * announces a bare number. All three visible states are checked, since each renders a
 * different fill and the "set aside" one is the state a user most needs read out.
 */
describe('AskCmdrContextGauge a11y', () => {
  beforeEach(() => {
    _setLocaleForTests('en-US')
  })

  async function expectClean(usage: ContextUsage): Promise<void> {
    const target = container()
    mount(AskCmdrContextGauge, { target, props: { usage } })
    flushSync()
    await expectNoA11yViolations(target)
    target.remove()
  }

  it('a calm gauge has no a11y violations', async () => {
    await expectClean({ estimatedTokens: 31_200, budgetTokens: 60_000, elidedResults: 0 })
  })

  it('a filling gauge has no a11y violations', async () => {
    await expectClean({ estimatedTokens: 50_000, budgetTokens: 60_000, elidedResults: 0 })
  })

  it('a set-aside gauge has no a11y violations', async () => {
    await expectClean({ estimatedTokens: 59_000, budgetTokens: 60_000, elidedResults: 3 })
  })
})

/**
 * Tier 3 a11y tests for `AskCmdrCostFooter.svelte`, the per-thread cost readout.
 *
 * The footer is a labelled row (token count + estimated cost) that only renders once the
 * thread has a metered turn. The trigger state and the cost command are mocked so it mounts
 * without a backend; a priced thread is used so the footer renders (its a11y surface).
 */
describe('AskCmdrCostFooter a11y', () => {
  beforeEach(() => {
    _setLocaleForTests('en-US')
    triggerState.conversationId = 1
  })

  it('the cost footer has no a11y violations', async () => {
    costMock.mockResolvedValue({
      promptTokens: 300,
      completionTokens: 70,
      costMicros: 1_230_000,
      fullyPriced: true,
      providers: ['openAi'],
    } satisfies ConversationCost)
    const target = container()
    mount(AskCmdrCostFooter, { target, props: {} })
    flushSync()
    await Promise.resolve()
    flushSync()
    await expectNoA11yViolations(target)
    target.remove()
  })
})

/**
 * Tier 3 a11y tests for `AskCmdrMessage.svelte`.
 *
 * One rendered thread item. Covers a user bubble, an assistant turn (tool lines +
 * "thinking…" + streaming markdown prose in a polite `aria-live` region), and a typed
 * failure notice. Takes its `message` as a prop, so no trigger-store wiring is needed.
 */
describe('AskCmdrMessage a11y', () => {
  function mountMessage(message: RailMessage): HTMLElement {
    const target = container()
    mount(AskCmdrMessage, { target, props: { message } })
    return target
  }

  it('a user bubble has no a11y violations', async () => {
    const target = mountMessage({ kind: 'user', id: 1, text: 'What is my biggest folder?', attachments: [] })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a user bubble with attachment chips has no a11y violations', async () => {
    const target = mountMessage({
      kind: 'user',
      id: 1,
      text: "What's in here?",
      attachments: [
        { path: '/Users/me/photos', kind: 'folder' },
        { path: '/Users/me/taxes.pdf', kind: 'file' },
      ],
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a streaming assistant turn with a tool line and thinking has no a11y violations', async () => {
    const target = mountMessage({
      kind: 'assistant',
      id: null,
      text: 'Your **Downloads** folder is the largest.',
      tools: [{ callId: 'c1', tool: 'largest_dirs', running: false, ok: true, path: '/Users/me' }],
      thinking: true,
      streaming: true,
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a finished assistant turn has no a11y violations', async () => {
    const target = mountMessage({
      kind: 'assistant',
      id: 5,
      text: 'Here is a list:\n\n- one\n- two',
      tools: [],
      thinking: false,
      streaming: false,
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a typed error notice has no a11y violations', async () => {
    const target = mountMessage({ kind: 'error', errorKind: 'rateLimited' })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('an error notice with provider detail has no a11y violations', async () => {
    const target = mountMessage({
      kind: 'error',
      errorKind: 'provider',
      detail: 'HTTP 404: This model is unavailable for free.',
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a model-change timeline line has no a11y violations', async () => {
    const target = mountMessage({ kind: 'modelChange', model: 'openai/gpt-oss-120b' })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `AskCmdrRail.svelte`.
 *
 * The whole rail: header (title + ALPHA badge + new-chat + close), the thread, the
 * soft-cap nudge, and the composer. Covers the empty state, a populated thread, and the
 * over-soft-cap nudge. The trigger store is mocked to a plain object so the rail + its
 * child composer mount without the full explorer-state chain.
 */
describe('AskCmdrRail a11y', () => {
  beforeEach(() => {
    railSettings['askCmdr.enabled'] = true
    railSettings['ai.provider'] = 'off'
    consentState.accepted = true
  })

  function mountRail(): HTMLElement {
    const target = container()
    mount(AskCmdrRail, { target, props: { readSelection: noSelection } })
    return target
  }

  function gateKind(target: HTMLElement): string | null {
    return target.querySelector('.ask-cmdr-gate')?.getAttribute('data-gate') ?? null
  }

  it('shows the off gate while Ask Cmdr is off, whatever the AI mode', async () => {
    railSettings['askCmdr.enabled'] = false
    railSettings['ai.provider'] = 'cloud'
    consentState.accepted = false
    const target = mountRail()
    await tick()
    expect(gateKind(target)).toBe('off')
    expect(target.querySelector('.composer')).toBeNull()
    target.remove()
  })

  it('shows the cloud gate on Cloud until cloud AI is allowed', async () => {
    railSettings['ai.provider'] = 'cloud'
    consentState.accepted = false
    const target = mountRail()
    await tick()
    expect(gateKind(target)).toBe('cloudOff')
    expect(target.querySelector('.composer')).toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('renders nothing while cloud consent is still loading, so no gate flashes', async () => {
    railSettings['ai.provider'] = 'cloud'
    consentState.accepted = null
    const target = mountRail()
    await tick()
    expect(gateKind(target)).toBeNull()
    expect(target.querySelector('.composer')).toBeNull()
    target.remove()
  })

  it('chats on Local with Ask Cmdr on: nothing leaves the Mac, so no consent is asked', async () => {
    railSettings['ai.provider'] = 'local'
    consentState.accepted = false
    const target = mountRail()
    await tick()
    expect(gateKind(target)).toBeNull()
    expect(target.querySelector('.composer')).not.toBeNull()
    target.remove()
  })

  it('the empty rail has no a11y violations', async () => {
    const target = mountRail()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a populated thread has no a11y violations', async () => {
    triggerState.messages = [
      { kind: 'user', id: 1, text: 'What is my biggest folder?', attachments: [] },
      {
        kind: 'assistant',
        id: 2,
        text: 'Your **Downloads** folder is the largest.',
        tools: [{ callId: 'c1', tool: 'largest_dirs', running: false, ok: true, path: '/Users/me' }],
        thinking: false,
        streaming: false,
      },
    ] satisfies RailMessage[]
    const target = mountRail()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('the over-soft-cap nudge has no a11y violations', async () => {
    triggerState.messages = [{ kind: 'user', id: 1, text: 'hi', attachments: [] }] satisfies RailMessage[]
    flags.overSoftCap = true
    const target = mountRail()
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `AskCmdrToolLine.svelte`.
 *
 * One collapsible "looked at X" line for a tool call. Covers the running state (a busy
 * status with a spinner), a finished-ok line with an expandable path, its expanded state,
 * and a refused line. `role="status"` + `aria-busy` and the toggle's `aria-expanded` are
 * the load-bearing attributes.
 */
describe('AskCmdrToolLine a11y', () => {
  function tool(overrides: Partial<RailToolCall> = {}): RailToolCall {
    return { callId: 'c1', tool: 'list_dir', running: false, ok: true, path: null, ...overrides }
  }

  function mountLine(t: RailToolCall): HTMLElement {
    const target = container()
    mount(AskCmdrToolLine, { target, props: { tool: t } })
    return target
  }

  it('a running tool line has no a11y violations', async () => {
    const target = mountLine(tool({ running: true }))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a finished line with a path has no a11y violations', async () => {
    const target = mountLine(tool({ path: '/Users/me/Documents' }))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('an expanded line has no a11y violations', async () => {
    const target = mountLine(tool({ path: '/Users/me/Documents' }))
    await tick()
    const toggle = target.querySelector<HTMLButtonElement>('.tool-toggle')
    if (toggle === null) throw new Error('expected a .tool-toggle button')
    toggle.click()
    await tick()
    expect(target.querySelector('.detail')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('a refused line has no a11y violations', async () => {
    const target = mountLine(tool({ ok: false }))
    await tick()
    await expectNoA11yViolations(target)
  })
})
