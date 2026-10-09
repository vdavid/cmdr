/**
 * The clash prompt answered from the keyboard, Total Commander style: one bare
 * letter per button, Enter = Skip, nothing for Escape. The shared guards
 * (modifiers, repeat, IME, text fields) are pinned in `../decision-keys.test.ts`;
 * this file pins the key map, the per-button disabled states, the arming delay,
 * and the focus the body reclaims. The hosts' forwarding is pinned beside them.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import TransferConflictDialog from './TransferConflictDialog.svelte'
import { CONFLICT_KEYS, DECISION_KEYS_ARM_MS } from '../decision-keys'
import type { WriteConflictEvent } from '$lib/tauri-commands'
import type { ConflictResolution } from '$lib/file-explorer/types'
import { buttonLabel } from '../test-button-label'

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

function clash(over: Partial<WriteConflictEvent> = {}): WriteConflictEvent {
  return {
    operationId: 'op-1',
    conflictId: 1,
    sourcePath: '/Users/test/src/report.pdf',
    destinationPath: '/Users/test/dest/report.pdf',
    sourceSize: 2048,
    destinationSize: 1024,
    sourceModified: 1_700_000_000,
    destinationModified: 1_699_000_000,
    destinationIsNewer: false,
    sizeDifference: -1024,
    sourceIsDirectory: false,
    destinationIsDirectory: false,
    ...over,
  }
}

const onResolve = vi.fn<(resolution: ConflictResolution, applyToAll: boolean) => void>()
const onCancel = vi.fn<(rollback: boolean) => void>()

/** Reactive, so a test can raise the next clash or flip a disabled state on the mounted body. */
const props = $state({
  conflictEvent: clash(),
  isCopy: true,
  isMove: false,
  rollbackUnavailable: false,
  isCancelling: false,
  isResolvingConflict: false,
})

let body: { handleKeydown(event: KeyboardEvent): boolean } | null = null
let host: HTMLElement | null = null

/** Mounts the body behind a stand-in host that forwards its keydowns, as both real hosts do. */
function render(): HTMLElement {
  host = document.createElement('div')
  document.body.appendChild(host)
  const instance = mount(TransferConflictDialog, {
    target: host,
    props: {
      get conflictEvent() {
        return props.conflictEvent
      },
      get isCopy() {
        return props.isCopy
      },
      get isMove() {
        return props.isMove
      },
      get rollbackUnavailable() {
        return props.rollbackUnavailable
      },
      get isCancelling() {
        return props.isCancelling
      },
      get isResolvingConflict() {
        return props.isResolvingConflict
      },
      onResolve,
      onCancel,
    },
  })
  mounted = instance
  body = instance as unknown as { handleKeydown(event: KeyboardEvent): boolean }
  host.addEventListener('keydown', (event) => body?.handleKeydown(event))
  flushSync()
  return host
}

/** Lets the prompt sit on screen long enough for its keys to go live. */
function arm(): void {
  vi.advanceTimersByTime(DECISION_KEYS_ARM_MS)
  flushSync()
}

function press(init: KeyboardEventInit, target?: Element | null): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init })
  ;(target ?? host?.querySelector('.conflict-section'))?.dispatchEvent(event)
  flushSync()
  return event
}

function letter(key: string): KeyboardEventInit {
  return { key: key.toLowerCase(), code: `Key${key}` }
}

let mounted: ReturnType<typeof mount> | null = null

beforeEach(() => {
  vi.useFakeTimers()
  document.body.innerHTML = ''
  onResolve.mockClear()
  onCancel.mockClear()
  props.conflictEvent = clash()
  props.isCopy = true
  props.isMove = false
  props.rollbackUnavailable = false
  props.isCancelling = false
  props.isResolvingConflict = false
})

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = null
  vi.useRealTimers()
})

describe('the letter for each answer', () => {
  it.each<[string, ConflictResolution, boolean]>([
    [CONFLICT_KEYS.skip, 'skip', false],
    [CONFLICT_KEYS.skipAll, 'skip', true],
    [CONFLICT_KEYS.rename, 'rename', false],
    [CONFLICT_KEYS.renameAll, 'rename', true],
    [CONFLICT_KEYS.overwrite, 'overwrite', false],
    [CONFLICT_KEYS.overwriteAll, 'overwrite', true],
    [CONFLICT_KEYS.overwriteAllSmaller, 'overwrite_smaller', true],
    [CONFLICT_KEYS.overwriteAllOlder, 'overwrite_older', true],
  ])('%s answers %s (apply to all: %s), exactly once', (key, resolution, applyToAll) => {
    render()
    arm()
    const event = press(letter(key))
    expect(onResolve).toHaveBeenCalledTimes(1)
    expect(onResolve).toHaveBeenCalledWith(resolution, applyToAll)
    expect(onCancel).not.toHaveBeenCalled()
    expect(event.defaultPrevented, 'claimed, so no shortcut runs it again').toBe(true)
  })

  it('answers through the physical key on a non-Latin layout', () => {
    render()
    arm()
    press({ key: 'щ', code: 'KeyO' })
    expect(onResolve).toHaveBeenCalledWith('overwrite', false)
  })

  it('shows each letter on its button and names it for a screen reader', () => {
    const target = render()
    const buttons = [...target.querySelectorAll('.conflict-buttons button')]
    const shown = buttons.map((button) => [
      buttonLabel(button),
      button.querySelector('kbd')?.textContent,
      button.getAttribute('aria-keyshortcuts'),
    ])
    expect(shown).toEqual([
      ['Skip', 'S', 'S Enter'],
      ['Skip all', 'K', 'K'],
      ['Rename', 'R', 'R'],
      ['Rename all', 'N', 'N'],
      ['Overwrite', 'O', 'O'],
      ['Overwrite all', 'A', 'A'],
      ['Overwrite all smaller', 'M', 'M'],
      ['Overwrite all older', 'L', 'L'],
    ])
  })
})

describe('Enter and Escape', () => {
  it('Enter skips this one file: the safe default, never an overwrite', () => {
    render()
    arm()
    press({ key: 'Enter', code: 'Enter' })
    expect(onResolve).toHaveBeenCalledTimes(1)
    expect(onResolve).toHaveBeenCalledWith('skip', false)
  })

  it('leaves Enter on a focused button to that button', () => {
    const target = render()
    arm()
    const overwriteButton = [...target.querySelectorAll('button')].find((b) => buttonLabel(b) === 'Overwrite')
    press({ key: 'Enter', code: 'Enter' }, overwriteButton)
    expect(onResolve).not.toHaveBeenCalled()
  })

  it('never answers on Escape: every way out of a clash is a decision', () => {
    render()
    arm()
    press({ key: 'Escape', code: 'Escape' })
    expect(onResolve).not.toHaveBeenCalled()
    expect(onCancel).not.toHaveBeenCalled()
  })
})

describe('the way out', () => {
  it('B asks to roll back a copy (the host asks first), and C does nothing where no Cancel shows', () => {
    const target = render()
    arm()
    press(letter(CONFLICT_KEYS.cancel))
    expect(onCancel).not.toHaveBeenCalled()
    press(letter(CONFLICT_KEYS.rollback))
    expect(onCancel).toHaveBeenCalledTimes(1)
    expect(onCancel).toHaveBeenCalledWith(true)
    expect(target.querySelector('.conflict-cancel button')?.getAttribute('aria-keyshortcuts')).toBe('B')
  })

  it('C cancels when the operation cannot roll back, and the blocked Rollback has no key', () => {
    props.rollbackUnavailable = true
    const target = render()
    arm()
    press(letter(CONFLICT_KEYS.rollback))
    expect(onCancel).not.toHaveBeenCalled()
    press(letter(CONFLICT_KEYS.cancel))
    expect(onCancel).toHaveBeenCalledTimes(1)
    expect(onCancel).toHaveBeenCalledWith(false)
    const blocked = target.querySelector('.conflict-cancel button[aria-disabled="true"]')
    expect(blocked?.querySelector('kbd')).toBeNull()
  })
})

describe('a key does what a click would, which for a disabled button is nothing', () => {
  it('ignores every answer while one is in flight', () => {
    render()
    arm()
    props.isResolvingConflict = true
    flushSync()
    for (const key of Object.values(CONFLICT_KEYS)) press(letter(key))
    press({ key: 'Enter', code: 'Enter' })
    expect(onResolve).not.toHaveBeenCalled()
    expect(onCancel).not.toHaveBeenCalled()
  })

  it('ignores the way out while a cancel is in flight, but still takes an answer', () => {
    render()
    arm()
    props.isCancelling = true
    flushSync()
    press(letter(CONFLICT_KEYS.rollback))
    expect(onCancel).not.toHaveBeenCalled()
    press(letter(CONFLICT_KEYS.skip))
    expect(onResolve).toHaveBeenCalledWith('skip', false)
  })

  it('ignores "Overwrite all smaller" when the destination size is unknown', () => {
    props.conflictEvent = clash({ destinationSize: null, sizeDifference: null })
    render()
    arm()
    press(letter(CONFLICT_KEYS.overwriteAllSmaller))
    expect(onResolve).not.toHaveBeenCalled()
  })

  it('ignores modified, repeated, and composing keypresses', () => {
    render()
    arm()
    press({ ...letter('O'), metaKey: true })
    press({ ...letter('O'), shiftKey: true })
    press({ ...letter('O'), repeat: true })
    press({ ...letter('O'), isComposing: true })
    expect(onResolve).not.toHaveBeenCalled()
  })
})

describe('a keystroke meant for something else', () => {
  it('cannot answer a clash that has only just appeared', () => {
    render()
    press(letter(CONFLICT_KEYS.overwrite))
    vi.advanceTimersByTime(DECISION_KEYS_ARM_MS - 1)
    flushSync()
    press(letter(CONFLICT_KEYS.overwrite))
    expect(onResolve).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1)
    flushSync()
    press(letter(CONFLICT_KEYS.overwrite))
    expect(onResolve).toHaveBeenCalledTimes(1)
  })

  it('re-arms for the next clash, so one keypress never answers two prompts', () => {
    render()
    arm()
    press(letter(CONFLICT_KEYS.skip))
    props.conflictEvent = clash({ conflictId: 2, destinationPath: '/Users/test/dest/other.pdf' })
    flushSync()
    press(letter(CONFLICT_KEYS.skip))
    expect(onResolve).toHaveBeenCalledTimes(1)
    arm()
    press(letter(CONFLICT_KEYS.skip))
    expect(onResolve).toHaveBeenCalledTimes(2)
  })
})

describe('focus', () => {
  it('takes focus when it was lost, so the keys reach a dialog', () => {
    const target = render()
    expect(document.activeElement).toBe(target.querySelector('.conflict-section'))
  })

  it('leaves focus alone when something else holds it', () => {
    const elsewhere = document.createElement('input')
    document.body.appendChild(elsewhere)
    elsewhere.focus()
    render()
    expect(document.activeElement).toBe(elsewhere)
  })
})
