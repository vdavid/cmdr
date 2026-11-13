import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AboutEvent, AboutFact, AboutPlan, Derived } from '#lib/ipc/bindings.js'
import {
  aboutThisState,
  actFromCard,
  openAboutThis,
  closeAboutThis,
  stopAboutVerdict,
  GRACE_MS,
  SHOW_ANYWAY_MS,
} from './about-this-state.svelte'
import { applyAboutEvent, emptyCard } from './about-card'

const { started } = vi.hoisted(() => ({
  started: [] as {
    requestId: string
    onEvent: (event: AboutEvent) => void
    cancel: ReturnType<typeof vi.fn>
    stopVerdict: ReturnType<typeof vi.fn>
  }[],
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  startAbout: (_item: unknown, onEvent: (event: AboutEvent) => void) => {
    const handle = {
      requestId: `req-${String(started.length + 1)}`,
      onEvent,
      cancel: vi.fn(() => Promise.resolve()),
      stopVerdict: vi.fn(() => Promise.resolve()),
    }
    started.push(handle)
    return handle
  },
}))

const plan: AboutPlan = {
  target: 'file',
  sections: [
    { section: 'origin', kinds: ['download'] },
    { section: 'nature', kinds: ['identity', 'realType'] },
    { section: 'removal', kinds: [] },
  ],
  volumeNote: null,
}

const derived: Derived = {
  headline: { what: { type: 'file', kind: null, extension: null }, size: null, lead: { template: 'plain' } },
  removal: { shown: [{ type: 'cantTell' }], more: [] },
  actions: { shown: [], more: [] },
}

const identity: AboutFact = {
  kind: 'identity',
  fact: {
    state: 'known',
    value: {
      name: 'a.txt',
      nameHidReordering: false,
      logicalSize: 12,
      physicalSize: 4096,
      created: 1,
      modified: 2,
      hardLinks: 1,
      symlinkTarget: null,
    },
  },
}
const noDownload: AboutFact = {
  kind: 'download',
  fact: { state: 'unknown', value: { why: 'noRecord', prominence: 'routine' } },
}

function planned(requestId: string): AboutEvent {
  return { requestId, update: { type: 'planned', plan, derived } }
}
function fact(requestId: string, f: AboutFact): AboutEvent {
  return { requestId, update: { type: 'fact', fact: f, derived } }
}
function noVerdict(requestId: string): AboutEvent {
  return { requestId, update: { type: 'verdictUnavailable', why: { kind: 'aiOff' } } }
}
function settled(requestId: string): AboutEvent {
  return { requestId, update: { type: 'factsSettled' } }
}

const subject = { path: '/tmp/a.txt', volumeId: 'root', name: 'a.txt', iconId: null, isDirectory: false }

describe('applyAboutEvent', () => {
  it('takes facts in any order once the plan is in', () => {
    let card = emptyCard('r', subject)
    card = applyAboutEvent(card, planned('r'))
    card = applyAboutEvent(card, fact('r', noDownload))
    card = applyAboutEvent(card, fact('r', identity))
    expect(card.facts.identity).toEqual(identity)
    expect(card.facts.download).toEqual(noDownload)
    expect(card.settled).toBe(false)
    card = applyAboutEvent(card, settled('r'))
    expect(card.settled).toBe(true)
  })

  it('folds the verdict in after the plan, and past the settle, without touching the facts', () => {
    const before = applyAboutEvent(emptyCard('r', subject), { requestId: 'r', update: { type: 'verdictPlanned' } })
    expect(before.verdict).toBeNull()
    let card = applyAboutEvent(emptyCard('r', subject), planned('r'))
    card = applyAboutEvent(card, settled('r'))
    const facts = card.facts
    card = applyAboutEvent(card, { requestId: 'r', update: { type: 'verdictPlanned' } })
    card = applyAboutEvent(card, { requestId: 'r', update: { type: 'verdictDelta', text: 'Words.' } })
    expect(card.verdict).toMatchObject({ status: 'planned', text: 'Words.' })
    expect(card.facts).toBe(facts)
    expect(applyAboutEvent(card, { requestId: 'other', update: { type: 'verdictDelta', text: 'x' } })).toBe(card)
  })

  it('ignores a fact outside the plan', () => {
    let card = applyAboutEvent(emptyCard('r', subject), planned('r'))
    const git: AboutFact = { kind: 'git', fact: { state: 'pending' } }
    const after = applyAboutEvent(card, fact('r', git))
    expect(after).toBe(card)
    card = after
    expect(card.facts.git).toBeUndefined()
  })

  it('ignores a fact before the plan, and another request’s events', () => {
    const card = emptyCard('r', subject)
    expect(applyAboutEvent(card, fact('r', identity))).toBe(card)
    expect(applyAboutEvent(card, planned('other'))).toBe(card)
  })

  it('ignores anything after the facts settled', () => {
    let card = applyAboutEvent(emptyCard('r', subject), planned('r'))
    card = applyAboutEvent(card, settled('r'))
    expect(applyAboutEvent(card, fact('r', identity))).toBe(card)
  })

  it('reads unavailable as the whole answer', () => {
    const card = applyAboutEvent(emptyCard('r', subject), {
      requestId: 'r',
      update: { type: 'unavailable', why: 'notFound' },
    })
    expect(card.unavailable).toBe('notFound')
    expect(card.settled).toBe(true)
    expect(applyAboutEvent(card, planned('r'))).toBe(card)
  })
})

describe('the open card', () => {
  beforeEach(() => {
    closeAboutThis()
    started.length = 0
  })

  it('shows the card at once and fills it from its own channel', () => {
    openAboutThis(subject)
    expect(aboutThisState.card?.plan).toBeNull()
    const [run] = started
    run.onEvent(planned(run.requestId))
    run.onEvent(fact(run.requestId, identity))
    expect(aboutThisState.card?.facts.identity).toEqual(identity)
  })

  it('shows the card on the first frame after the grace, with what answered inside it', () => {
    vi.useFakeTimers()
    try {
      openAboutThis(subject)
      const [run] = started
      run.onEvent(planned(run.requestId))
      run.onEvent(noVerdict(run.requestId))
      expect(aboutThisState.shown).toBe(false)
      vi.advanceTimersByTime(GRACE_MS)
      expect(aboutThisState.shown).toBe(true)
    } finally {
      vi.useRealTimers()
    }
  })

  it('waits for the verdict’s answer as well as the plan, so its place is settled on the first frame', () => {
    vi.useFakeTimers()
    try {
      openAboutThis(subject)
      const [run] = started
      vi.advanceTimersByTime(GRACE_MS)
      run.onEvent(planned(run.requestId))
      expect(aboutThisState.shown).toBe(false)
      run.onEvent({ requestId: run.requestId, update: { type: 'verdictPlanned' } })
      expect(aboutThisState.shown).toBe(true)
    } finally {
      vi.useRealTimers()
    }
  })

  it('waits past the grace for a slow plan, and shows anyway after the cap', () => {
    vi.useFakeTimers()
    try {
      openAboutThis(subject)
      vi.advanceTimersByTime(GRACE_MS)
      expect(aboutThisState.shown).toBe(false)
      const [run] = started
      run.onEvent(planned(run.requestId))
      run.onEvent(noVerdict(run.requestId))
      expect(aboutThisState.shown).toBe(true)

      openAboutThis({ ...subject, path: '/tmp/slow' })
      vi.advanceTimersByTime(SHOW_ANYWAY_MS)
      expect(aboutThisState.shown).toBe(true)
    } finally {
      vi.useRealTimers()
    }
  })

  it('cancels on close, and a late event after close changes nothing', () => {
    openAboutThis(subject)
    const [run] = started
    run.onEvent(planned(run.requestId))
    closeAboutThis()
    expect(run.cancel).toHaveBeenCalledOnce()
    run.onEvent(fact(run.requestId, identity))
    expect(aboutThisState.card).toBeNull()
  })

  it('never shows the first item’s facts after a reopen on another item', () => {
    openAboutThis(subject)
    const [first] = started
    openAboutThis({ ...subject, path: '/tmp/b.txt', name: 'b.txt' })
    const [, second] = started
    expect(first.cancel).toHaveBeenCalledOnce()
    first.onEvent(planned(first.requestId))
    first.onEvent(fact(first.requestId, identity))
    expect(aboutThisState.card?.plan).toBeNull()
    expect(aboutThisState.card?.facts.identity).toBeUndefined()
    second.onEvent(planned(second.requestId))
    expect(aboutThisState.card?.subject.name).toBe('b.txt')
    expect(aboutThisState.card?.plan).toEqual(plan)
  })

  it('stops the verdict alone and keeps the card, and a close stops both', () => {
    openAboutThis(subject)
    const [run] = started
    run.onEvent(planned(run.requestId))
    run.onEvent({ requestId: run.requestId, update: { type: 'verdictPlanned' } })
    run.onEvent({ requestId: run.requestId, update: { type: 'verdictDelta', text: 'Half a ' } })
    stopAboutVerdict()
    expect(run.stopVerdict).toHaveBeenCalledOnce()
    expect(run.cancel).not.toHaveBeenCalled()
    run.onEvent({ requestId: run.requestId, update: { type: 'verdictEnd', ending: { kind: 'stopped' }, cost: null } })
    expect(aboutThisState.card?.verdict).toMatchObject({ text: 'Half a ', ending: { kind: 'stopped' } })
    closeAboutThis()
    expect(run.cancel).toHaveBeenCalledOnce()
    stopAboutVerdict()
    expect(run.stopVerdict).toHaveBeenCalledOnce()
  })

  it('never starts a verdict by itself: a card refused for consent stays refused until the next open', () => {
    openAboutThis(subject)
    const [run] = started
    run.onEvent(planned(run.requestId))
    run.onEvent({ requestId: run.requestId, update: { type: 'verdictUnavailable', why: { kind: 'noCloudConsent' } } })
    // Consent arriving while it's open is a settings change the card doesn't listen to: no second request.
    run.onEvent({ requestId: run.requestId, update: { type: 'verdictPlanned' } })
    expect(started).toHaveLength(1)
    expect(aboutThisState.card?.verdict).toEqual({ status: 'unavailable', why: { kind: 'noCloudConsent' } })
  })

  it('closes the card before an action runs, so focus lands where the action went', async () => {
    const seen: { open: boolean; type: string; path: string }[] = []
    openAboutThis(subject, {
      goTo: vi.fn(),
      act: (action, about) => {
        seen.push({ open: aboutThisState.card !== null, type: action.type, path: about.path })
      },
    })
    const [run] = started
    actFromCard({ type: 'moveToTrash' })
    expect(run.cancel).toHaveBeenCalledOnce()
    // The action waits for the dialog to unmount: until then the delete flow would see it still open.
    expect(seen).toEqual([])
    await vi.waitFor(() => {
      expect(seen).toHaveLength(1)
    })
    expect(seen).toEqual([{ open: false, type: 'moveToTrash', path: subject.path }])
    // With no card up, nothing acts.
    actFromCard({ type: 'moveToTrash' })
    await Promise.resolve()
    expect(seen).toHaveLength(1)
  })
})

describe('a volume that stopped answering', () => {
  const onShare: AboutPlan = { ...plan, volumeNote: { volume: 'Naspolya', limit: 'someFacts' } }

  it('turns the note into its stopped words and keeps every row', () => {
    let card = applyAboutEvent(emptyCard('r', subject), {
      requestId: 'r',
      update: { type: 'planned', plan: onShare, derived },
    })
    card = applyAboutEvent(card, { requestId: 'r', update: { type: 'volumeStoppedAnswering' } })
    expect(card.plan?.volumeNote).toEqual({ volume: 'Naspolya', limit: 'unreachable' })
    expect(card.plan?.sections).toEqual(onShare.sections)
  })

  it('changes nothing before the plan, after the settle, or for another card', () => {
    const stopped: AboutEvent = { requestId: 'r', update: { type: 'volumeStoppedAnswering' } }
    const empty = emptyCard('r', subject)
    expect(applyAboutEvent(empty, stopped)).toBe(empty)
    let card = applyAboutEvent(empty, { requestId: 'r', update: { type: 'planned', plan: onShare, derived } })
    expect(applyAboutEvent(card, { ...stopped, requestId: 'other' })).toBe(card)
    card = applyAboutEvent(card, settled('r'))
    expect(applyAboutEvent(card, stopped)).toBe(card)
  })
})
