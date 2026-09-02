/**
 * Tier 3 a11y tests for the Ask Cmdr rail and the pieces it renders: the opt-in
 * gate, the composer, a thread message, a tool line, an attachment chip, the
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
import { _setLocaleForTests } from '$lib/intl/locale'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { AttachmentRef, ConversationCost } from '$lib/tauri-commands'
import type { RailMessage, RailToolCall } from './ask-cmdr-trigger.svelte'
import type { ContextUsage } from './ask-cmdr-context-usage'

// `vi.hoisted` so the shared mutable state exists before the hoisted `vi.mock`
// factories run.
const { triggerState, flags, costMock, consentState } = vi.hoisted(() => ({
  // A plain mutable object, so the consent block can move `needsReconsent` before mounting.
  consentState: { accepted: true, acceptedAt: null, needsReconsent: false },
  triggerState: {
    streaming: false,
    width: 340,
    conversationId: null as number | null,
    messages: [] as unknown[],
    attachments: [] as unknown[],
  },
  flags: { overSoftCap: false },
  costMock: vi.fn<(id: number) => Promise<unknown>>(),
}))

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
  newChat: vi.fn(),
  setRailWidth: vi.fn(),
  sendMessage: vi.fn(),
  stopStreaming: vi.fn(),
  markRailFocused: vi.fn(),
  returnFocusToPane: vi.fn(),
  addAttachments: vi.fn(),
  removeAttachment: vi.fn(),
}))

// Consent granted so the rail renders the chat (not the opt-in gate).
vi.mock('./ask-cmdr-consent.svelte', () => ({
  consentState,
  refreshConsent: vi.fn(),
  acceptConsent: vi.fn(() => Promise.resolve(true)),
  revokeConsent: vi.fn(),
}))

vi.mock('./ask-cmdr-sessions.svelte', () => ({
  sessionsState: { open: false },
  openSessions: vi.fn(),
}))

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  askCmdrConversationCost: (id: number) => costMock(id),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import AskCmdrAttachmentChip from './AskCmdrAttachmentChip.svelte'
import AskCmdrComposer from './AskCmdrComposer.svelte'
import AskCmdrConsent from './AskCmdrConsent.svelte'
import AskCmdrContextGauge from './AskCmdrContextGauge.svelte'
import AskCmdrCostFooter from './AskCmdrCostFooter.svelte'
import AskCmdrMessage from './AskCmdrMessage.svelte'
import AskCmdrRail from './AskCmdrRail.svelte'
import AskCmdrToolLine from './AskCmdrToolLine.svelte'
import { getMessage } from '$lib/intl/messages.svelte'

/** A fresh container, appended to the document and ready to mount into. */
function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

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
    mount(AskCmdrComposer, { target, props: {} })
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
 * Tier 3 a11y tests for `AskCmdrConsent.svelte`, the opt-in gate, plus the one branch it
 * owns: the "here's what changed" block a returning user gets and a first-time user doesn't.
 *
 * The screen: a labelled group (heading + intro + the "what leaves your Mac" list + the
 * reassurance paragraphs + the local-storage note), and the two actions (Not now / Turn on).
 * The consent + trigger modules are mocked so it mounts without a backend.
 */
describe('AskCmdrConsent a11y', () => {
  async function mountGate(): Promise<HTMLElement> {
    const target = container()
    mount(AskCmdrConsent, { target, props: {} })
    await tick()
    return target
  }

  beforeEach(() => {
    consentState.needsReconsent = false
  })

  it('the opt-in gate has no a11y violations', async () => {
    const target = await mountGate()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('the re-prompt has no a11y violations either', async () => {
    consentState.needsReconsent = true
    const target = await mountGate()
    await expectNoA11yViolations(target)
    target.remove()
  })

  /**
   * The disclosure the whole re-prompt exists for. Without it the bump collects a signature
   * on the old promise, which is the one thing a consent bump must not do.
   */
  it('always discloses the memory the agent keeps and sends', async () => {
    const target = await mountGate()
    expect(target.textContent).toContain(getMessage('askCmdr.consent.item.memory'))
    expect(target.textContent).toContain(getMessage('askCmdr.consent.memory'))
    expect(target.textContent).toContain(getMessage('askCmdr.consent.proactive'))
    target.remove()
  })

  /** ❌ The old promise must not come back: the agent proposes changes and writes its notes. */
  it('no longer claims the agent never changes anything', async () => {
    const target = await mountGate()
    expect(target.textContent).not.toContain('never changes anything')
    target.remove()
  })

  /**
   * The disclosure the copy-version-4 re-prompt exists for: `inspect_file` reads parts of a
   * file on request (text, PDF pages, archive entries, a photo's EXIF with its location), so
   * the list names it beside names and sizes, and the reassurance paragraph must not promise
   * "no file contents" any more.
   */
  it('discloses that parts of files are read on request', async () => {
    const target = await mountGate()
    expect(target.textContent).toContain(getMessage('askCmdr.consent.item.contents'))
    expect(target.textContent).toContain(getMessage('askCmdr.consent.contentsRule'))
    expect(target.textContent).not.toContain('no file contents')
    target.remove()
  })

  it('says nothing about a change to somebody who is opting in for the first time', async () => {
    const target = await mountGate()
    expect(target.textContent).not.toContain(getMessage('askCmdr.consent.whatsNew.title'))
    target.remove()
  })

  /**
   * Somebody whose opt-in the copy bump revoked has a whole thread history sitting behind this
   * screen. Showing them the first-run pitch with no reason for it reads as the app losing it.
   */
  it('tells a returning user what changed', async () => {
    consentState.needsReconsent = true
    const target = await mountGate()
    expect(target.textContent).toContain(getMessage('askCmdr.consent.whatsNew.title'))
    expect(target.textContent).toContain(getMessage('askCmdr.consent.whatsNew.body'))
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
  function mountRail(): HTMLElement {
    const target = container()
    mount(AskCmdrRail, { target, props: {} })
    return target
  }

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
