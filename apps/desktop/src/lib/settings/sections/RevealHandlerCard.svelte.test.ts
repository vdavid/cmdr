/**
 * Tier-3 tests for `RevealHandlerCard.svelte` (Behavior › Navigation & file ops).
 *
 * The card is the one settings row with no registry entry behind it: what it
 * shows comes from the macOS `NSFileViewer` preference, read through IPC on every
 * mount. So what's pinned here is the state → rendering map, and that the row
 * renders the state the OS was LEFT in rather than the one the click asked for.
 *
 * `$lib/ipc/bindings` is mocked rather than `$lib/tauri-commands`, so the real
 * `getRevealHandlerState` / `setRevealHandlerEnabled` wrappers run: their
 * swallow-and-report-`unavailable` fallback is half of why a non-macOS build
 * renders nothing, and mocking above them would skip it.
 *
 * ❗ What this file CANNOT cover: the row hides itself on every non-production
 * build, and the E2E suite only ever runs those, so there is no end-to-end test
 * of it. `settings.spec.ts` would find an empty section and pass.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { RevealHandlerState, RevealHandlerStatus } from '$lib/ipc/bindings'

const getRevealHandlerState = vi.fn<() => Promise<RevealHandlerStatus>>()
const setRevealHandlerEnabled = vi.fn<(enabled: boolean) => Promise<RevealHandlerStatus>>()
const isMacOS = vi.fn<() => boolean>()

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    getRevealHandlerState: () => getRevealHandlerState(),
    setRevealHandlerEnabled: (enabled: boolean) => setRevealHandlerEnabled(enabled),
  },
}))

vi.mock('$lib/shortcuts/key-capture', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/shortcuts/key-capture')>()),
  isMacOS: () => isMacOS(),
}))

import RevealHandlerCard from './RevealHandlerCard.svelte'

async function mountCard(searchQuery = ''): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(RevealHandlerCard, { target, props: { searchQuery } })
  await tick()
  await tick()
  return target
}

function toggle(target: HTMLElement): HTMLInputElement | null {
  return target.querySelector('[data-test="reveal-handler-switch"]')
}

/** The whole card, or `null` when it renders nothing at all. */
function card(target: HTMLElement): HTMLElement | null {
  return target.querySelector('.reveal-row')
}

/** A status with nothing standing in the way of the switch. */
function unblocked(state: RevealHandlerState): RevealHandlerStatus {
  return { state, blockedBy: null }
}

/** The same, from a copy of Cmdr that isn't in an Applications folder. */
function blocked(state: RevealHandlerState): RevealHandlerStatus {
  return { state, blockedBy: 'notInApplications' }
}

const HELD_BY_PATH_FINDER: RevealHandlerState = {
  kind: 'heldByOtherApp',
  bundleId: 'com.cocoatech.PathFinder',
  displayName: 'Path Finder',
}

beforeEach(() => {
  vi.clearAllMocks()
  isMacOS.mockReturnValue(true)
  getRevealHandlerState.mockResolvedValue(unblocked({ kind: 'notRegistered' }))
})

describe('what the card shows for each handler state', () => {
  it('renders the switch on when Cmdr holds the key', async () => {
    getRevealHandlerState.mockResolvedValue(unblocked({ kind: 'registered' }))

    const target = await mountCard()

    expect(card(target)).not.toBeNull()
    expect(toggle(target)?.checked).toBe(true)
  })

  it('renders the switch off, and says nothing about a holder, when nobody holds the key', async () => {
    const target = await mountCard()

    expect(toggle(target)?.checked).toBe(false)
    expect(target.querySelector('.reveal-holder')).toBeNull()
  })

  it('names the app that currently holds the key, with the switch off', async () => {
    getRevealHandlerState.mockResolvedValue(unblocked(HELD_BY_PATH_FINDER))

    const target = await mountCard()

    expect(toggle(target)?.checked).toBe(false)
    expect(target.querySelector('.reveal-holder')?.textContent).toContain('Path Finder')
  })

  it('falls back to the raw bundle id when the holder is not installed any more', async () => {
    getRevealHandlerState.mockResolvedValue(
      unblocked({ kind: 'heldByOtherApp', bundleId: 'com.binarynights.ForkLift-3', displayName: null }),
    )

    const target = await mountCard()

    expect(target.querySelector('.reveal-holder')?.textContent).toContain('com.binarynights.ForkLift-3')
  })
})

describe('when the card renders nothing at all', () => {
  it('stays away on a build that must never write the key', async () => {
    getRevealHandlerState.mockResolvedValue(unblocked({ kind: 'unavailable' }))

    const target = await mountCard()

    expect(card(target)).toBeNull()
  })

  it('stays away when the IPC command is missing, as it is off macOS', async () => {
    // The wrapper turns the rejection into `unavailable`; nothing reaches the user.
    getRevealHandlerState.mockRejectedValue(new Error('command not found'))

    const target = await mountCard()

    expect(card(target)).toBeNull()
  })

  it('does not even ask the backend when this is not a Mac', async () => {
    isMacOS.mockReturnValue(false)

    const target = await mountCard()

    expect(card(target)).toBeNull()
    expect(getRevealHandlerState).not.toHaveBeenCalled()
  })

  it('hides under any search query, because the row is not in the search index', async () => {
    getRevealHandlerState.mockResolvedValue(unblocked({ kind: 'registered' }))

    const target = await mountCard('finder')

    expect(card(target)).toBeNull()
  })
})

describe('turning the handler on and off', () => {
  it('takes the key over from another app on one click', async () => {
    getRevealHandlerState.mockResolvedValue(unblocked(HELD_BY_PATH_FINDER))
    setRevealHandlerEnabled.mockResolvedValue(unblocked({ kind: 'registered' }))

    const target = await mountCard()
    toggle(target)?.click()
    await tick()
    await tick()

    expect(setRevealHandlerEnabled).toHaveBeenCalledWith(true)
    expect(toggle(target)?.checked).toBe(true)
    expect(target.querySelector('.reveal-holder')).toBeNull()
  })

  it('hands the key back when switched off', async () => {
    getRevealHandlerState.mockResolvedValue(unblocked({ kind: 'registered' }))
    setRevealHandlerEnabled.mockResolvedValue(unblocked({ kind: 'notRegistered' }))

    const target = await mountCard()
    toggle(target)?.click()
    await tick()
    await tick()

    expect(setRevealHandlerEnabled).toHaveBeenCalledWith(false)
    expect(toggle(target)?.checked).toBe(false)
  })

  it('renders the state the OS was left in, not the one the click asked for', async () => {
    // Another app grabbed the key between the read and the click, so the write
    // touched nothing and the row has to say so instead of showing an on switch.
    setRevealHandlerEnabled.mockResolvedValue(unblocked(HELD_BY_PATH_FINDER))

    const target = await mountCard()
    toggle(target)?.click()
    await tick()
    await tick()

    expect(toggle(target)?.checked).toBe(false)
    expect(target.querySelector('.reveal-holder')?.textContent).toContain('Path Finder')
  })
})

/**
 * The warning nothing else can deliver: macOS keeps routing "Show in Finder" to a
 * bundle id that no longer exists, and nothing of ours runs at uninstall to clear
 * it. Its whole value is being on screen at the moment the switch is on.
 */
describe('the uninstall warning', () => {
  function warning(target: HTMLElement): HTMLElement | null {
    return target.querySelector('[data-test="reveal-handler-warning"]')
  }

  it('warns while the handler is on', async () => {
    getRevealHandlerState.mockResolvedValue(unblocked({ kind: 'registered' }))

    const target = await mountCard()

    expect(warning(target)).not.toBeNull()
  })

  it('says nothing before the switch is on, where it would be friction', async () => {
    const target = await mountCard()

    expect(warning(target)).toBeNull()
  })

  it('says nothing while another app holds the key', async () => {
    getRevealHandlerState.mockResolvedValue(unblocked(HELD_BY_PATH_FINDER))

    const target = await mountCard()

    expect(warning(target)).toBeNull()
  })

  it('appears the moment the switch is turned on, and goes on turning it off', async () => {
    setRevealHandlerEnabled.mockResolvedValue(unblocked({ kind: 'registered' }))

    const target = await mountCard()
    toggle(target)?.click()
    await tick()
    await tick()
    expect(warning(target)).not.toBeNull()

    setRevealHandlerEnabled.mockResolvedValue(unblocked({ kind: 'notRegistered' }))
    toggle(target)?.click()
    await tick()
    await tick()

    expect(warning(target)).toBeNull()
  })
})

/**
 * The Applications-folder gate. ❗ It blocks turning the handler ON and never
 * turning it OFF: a copy that holds the key and then moves out of Applications
 * must still be able to hand it back, or the person is stranded holding the
 * dangling key the gate exists to prevent.
 */
describe('a copy of Cmdr outside an Applications folder', () => {
  function blockedReason(target: HTMLElement): HTMLElement | null {
    return target.querySelector('[data-test="reveal-handler-blocked"]')
  }

  it('cannot switch the handler on, and says why', async () => {
    getRevealHandlerState.mockResolvedValue(blocked({ kind: 'notRegistered' }))

    const target = await mountCard()

    expect(toggle(target)?.disabled).toBe(true)
    // The real catalog resolves here (nothing mocks `$lib/intl`), so this pins the one
    // thing the sentence has to carry: what to do about it.
    expect(blockedReason(target)?.textContent).toContain('Applications folder')
  })

  it('cannot take the key from another app either', async () => {
    getRevealHandlerState.mockResolvedValue(blocked(HELD_BY_PATH_FINDER))

    const target = await mountCard()

    expect(toggle(target)?.disabled).toBe(true)
    expect(target.querySelector('.reveal-holder')?.textContent).toContain('Path Finder')
  })

  it('can still switch a handler it already holds back off', async () => {
    // The backend reports no blocker while the key is ours, exactly so this stays possible.
    getRevealHandlerState.mockResolvedValue(unblocked({ kind: 'registered' }))
    setRevealHandlerEnabled.mockResolvedValue(blocked({ kind: 'notRegistered' }))

    const target = await mountCard()
    expect(toggle(target)?.disabled).toBe(false)

    toggle(target)?.click()
    await tick()
    await tick()

    expect(setRevealHandlerEnabled).toHaveBeenCalledWith(false)
    expect(toggle(target)?.checked).toBe(false)
    // And now that it's handed back, the row locks: this copy may not take it again.
    expect(toggle(target)?.disabled).toBe(true)
  })

  it('leaves the switch alone where nothing is blocking it', async () => {
    const target = await mountCard()

    expect(toggle(target)?.disabled).toBe(false)
    expect(blockedReason(target)).toBeNull()
  })
})
