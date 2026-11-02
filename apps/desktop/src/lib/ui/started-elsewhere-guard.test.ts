import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  ARM_QUIET_MS,
  PRESS_IGNORE_MS,
  attachStartedElsewhereGuard,
  createGuardMachine,
  type GuardKey,
  type GuardModifier,
} from './started-elsewhere-guard'

vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

function key(name: string, options: { repeat?: boolean; modifiers?: GuardModifier[] } = {}): GuardKey {
  return { id: name, key: name, repeat: options.repeat ?? false, modifiers: options.modifiers ?? [] }
}

/** A machine that's already quiet long enough, and armed. */
function armedMachine() {
  const machine = createGuardMachine(0)
  machine.tick(ARM_QUIET_MS)
  expect(machine.armed).toBe(true)
  return machine
}

describe('the state machine', () => {
  it('starts unarmed and arms after the quiet window', () => {
    const machine = createGuardMachine(0)
    expect(machine.armed).toBe(false)
    expect(machine.tick(ARM_QUIET_MS - 1)).toBe(false)
    expect(machine.tick(ARM_QUIET_MS)).toBe(true)
    expect(machine.armed).toBe(true)
  })

  it('never arms during a typing stream, and arms once it stops', () => {
    const machine = createGuardMachine(0)
    let now = 0
    for (const letter of 'hello there, could you') {
      now += 300
      expect(machine.keyDown(key(letter), now)).toBe('swallow')
      machine.tick(now + 50)
      expect(machine.keyUp(key(letter), now + 50)).toBe('swallow')
      now += 50
      expect(machine.tick(now)).toBe(false)
    }
    expect(machine.armed).toBe(false)
    expect(machine.tick(now + ARM_QUIET_MS)).toBe(true)
  })

  it('swallows Enter at open', () => {
    const machine = createGuardMachine(0)
    expect(machine.keyDown(key('Enter'), 1)).toBe('swallow')
    expect(machine.keyUp(key('Enter'), 40)).toBe('swallow')
  })

  it('swallows a held Enter, whose repeats keep it from arming', () => {
    const machine = createGuardMachine(0)
    machine.keyDown(key('Enter'), 10)
    for (let now = 500; now < 5000; now += 33) {
      expect(machine.keyDown(key('Enter', { repeat: true }), now)).toBe('swallow')
      expect(machine.tick(now + 1)).toBe(false)
    }
    machine.keyUp(key('Enter'), 5000)
    expect(machine.tick(5000 + ARM_QUIET_MS - 1)).toBe(false)
    expect(machine.tick(5000 + ARM_QUIET_MS)).toBe(true)
  })

  it('takes down the guard for a key held since before it, once its repeats show', () => {
    const machine = armedMachine()
    expect(machine.keyDown(key('Enter', { repeat: true }), 900)).toBe('swallow')
    expect(machine.armed).toBe(false)
  })

  it('lets Enter through after the quiet window', () => {
    expect(armedMachine().keyDown(key('Enter'), 900)).toBe('pass')
  })

  it('never swallows Escape, armed or not', () => {
    const unarmed = createGuardMachine(0)
    expect(unarmed.keyDown(key('Escape'), 1)).toBe('pass')
    expect(unarmed.keyUp(key('Escape'), 2)).toBe('pass')
    expect(armedMachine().keyDown(key('Escape'), 900)).toBe('pass')
  })

  it('ignores a primary-button press for the first moments, and takes one after', () => {
    const machine = createGuardMachine(0)
    expect(machine.press(PRESS_IGNORE_MS - 1)).toBe('swallow')
    expect(machine.gestured).toBe(false)
    expect(machine.press(PRESS_IGNORE_MS)).toBe('pass')
    expect(machine.gestured).toBe(true)
  })

  it('never arms under a Shift held from inside the guard, since a modifier sends no repeats', () => {
    const machine = createGuardMachine(0)
    machine.keyDown(key('Shift', { modifiers: ['Shift'] }), 10)
    expect(machine.tick(10_000)).toBe(false)
    machine.keyUp(key('Shift'), 10_000)
    expect(machine.tick(10_000 + ARM_QUIET_MS)).toBe(true)
  })

  it('catches a Shift held from before the guard, on the first key that shows it', () => {
    const machine = armedMachine()
    expect(machine.keyDown(key('Enter', { modifiers: ['Shift'] }), 900)).toBe('swallow')
    expect(machine.armed).toBe(false)
    machine.keyUp(key('Enter', { modifiers: ['Shift'] }), 950)
    expect(machine.tick(950 + ARM_QUIET_MS)).toBe(false)
    machine.keyUp(key('Shift'), 2000)
    expect(machine.tick(2000 + ARM_QUIET_MS)).toBe(true)
  })

  it('lets a Shift pressed after arming through, since nobody forgot it', () => {
    const machine = armedMachine()
    expect(machine.keyDown(key('Shift', { modifiers: ['Shift'] }), 900)).toBe('pass')
    expect(machine.keyDown(key('Enter', { modifiers: ['Shift'] }), 950)).toBe('pass')
  })

  it('holds text back after arming, until a click or a Tab', () => {
    const clicked = armedMachine()
    expect(clicked.keyDown(key('a'), 900)).toBe('swallow')
    expect(clicked.textAllowed).toBe(false)
    expect(clicked.press(1000)).toBe('pass')
    expect(clicked.textAllowed).toBe(true)
    expect(clicked.keyDown(key('b'), 1100)).toBe('pass')

    const tabbed = armedMachine()
    expect(tabbed.keyDown(key('Tab'), 900)).toBe('unlocked')
    expect(tabbed.textAllowed).toBe(true)
    expect(tabbed.keyDown(key('c'), 1000)).toBe('pass')
    expect(tabbed.keyDown(key('Tab'), 1100)).toBe('pass')
  })

  it("doesn't count an unarmed Tab as the gesture", () => {
    const machine = createGuardMachine(0)
    expect(machine.keyDown(key('Tab'), 1)).toBe('swallow')
    expect(machine.gestured).toBe(false)
  })

  it('lets go of what the window took with it', () => {
    const machine = createGuardMachine(0)
    machine.keyDown(key('a'), 1)
    machine.windowLeft(2)
    expect(machine.tick(2 + ARM_QUIET_MS)).toBe(true)
  })

  it('forgets a key released under Command, whose keyup macOS never sends', () => {
    const machine = createGuardMachine(0)
    machine.keyDown(key('Meta', { modifiers: ['Meta'] }), 1)
    machine.keyDown(key('c', { modifiers: ['Meta'] }), 2)
    machine.keyUp(key('Meta'), 3)
    expect(machine.tick(3 + ARM_QUIET_MS)).toBe(true)
  })

  it("doesn't read an on Caps Lock as a key held down", () => {
    const machine = createGuardMachine(0)
    machine.keyDown(key('CapsLock'), 1)
    expect(machine.tick(1 + ARM_QUIET_MS)).toBe(true)
  })

  it('counts an IME composing as someone at the keyboard', () => {
    const machine = createGuardMachine(0)
    machine.activity(700)
    expect(machine.tick(ARM_QUIET_MS)).toBe(false)
    expect(machine.tick(700 + ARM_QUIET_MS)).toBe(true)
  })
})

describe('the listener', () => {
  let scope: HTMLDivElement
  let park: HTMLDivElement
  let field: HTMLInputElement
  let onArmedChange: ReturnType<typeof vi.fn<(armed: boolean) => void>>
  let guard: ReturnType<typeof attachStartedElsewhereGuard>

  function attach(tabMovesFocus = true): void {
    guard = attachStartedElsewhereGuard({
      scope: () => scope,
      parkFocus: () => park,
      tabMovesFocus,
      onArmedChange,
    })
  }

  function press(keyName: string, init: KeyboardEventInit = {}): KeyboardEvent {
    const event = new KeyboardEvent('keydown', { key: keyName, bubbles: true, cancelable: true, ...init })
    ;(document.activeElement ?? document.body).dispatchEvent(event)
    return event
  }

  /** What a handler on the page would see. */
  let reached: string[]
  const record = (event: Event) => {
    reached.push(event instanceof KeyboardEvent ? event.key : event.type)
  }

  beforeEach(() => {
    vi.useFakeTimers()
    reached = []
    onArmedChange = vi.fn<(armed: boolean) => void>()
    scope = document.createElement('div')
    park = document.createElement('div')
    park.tabIndex = -1
    field = document.createElement('input')
    scope.append(park, field)
    document.body.append(scope)
    scope.addEventListener('keydown', record)
    scope.addEventListener('click', record)
    scope.addEventListener('beforeinput', record)
    park.focus()
  })

  afterEach(() => {
    guard.detach()
    scope.remove()
    vi.useRealTimers()
  })

  it('arms on the timer and says so', () => {
    attach()
    vi.advanceTimersByTime(ARM_QUIET_MS - 1)
    expect(onArmedChange).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1)
    expect(onArmedChange).toHaveBeenCalledExactlyOnceWith(true)
  })

  it('a key restarts the clock and never reaches the page while unarmed', () => {
    attach()
    vi.advanceTimersByTime(ARM_QUIET_MS - 1)
    const event = press('Enter')
    window.dispatchEvent(new KeyboardEvent('keyup', { key: 'Enter' }))
    expect(reached).toEqual([])
    expect(event.defaultPrevented).toBe(true)
    vi.advanceTimersByTime(ARM_QUIET_MS - 1)
    expect(onArmedChange).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1)
    expect(onArmedChange).toHaveBeenCalledExactlyOnceWith(true)
    press('Enter')
    expect(reached).toEqual(['Enter'])
  })

  it('Escape always reaches the page', () => {
    attach()
    press('Escape')
    expect(reached).toEqual(['Escape'])
  })

  it('swallows a click on the first moments, and the press that started it', () => {
    attach()
    const down = new MouseEvent('pointerdown', { button: 0, bubbles: true, cancelable: true })
    field.dispatchEvent(down)
    expect(down.defaultPrevented).toBe(true)
    field.dispatchEvent(new MouseEvent('click', { button: 0, bubbles: true }))
    expect(reached).toEqual([])
  })

  it('lets a click through after the first moments', () => {
    attach()
    vi.advanceTimersByTime(PRESS_IGNORE_MS)
    field.dispatchEvent(new MouseEvent('pointerdown', { button: 0, bubbles: true, cancelable: true }))
    field.dispatchEvent(new MouseEvent('click', { button: 0, bubbles: true }))
    expect(reached).toEqual(['click'])
  })

  it('takes focus back to where it parks when something focuses a field on its own', () => {
    attach()
    field.focus()
    expect(document.activeElement).toBe(park)
  })

  it('lets focus reach a field after a click', () => {
    attach()
    vi.advanceTimersByTime(PRESS_IGNORE_MS)
    field.dispatchEvent(new MouseEvent('pointerdown', { button: 0, bubbles: true, cancelable: true }))
    field.focus()
    expect(document.activeElement).toBe(field)
  })

  it('holds back paste and dictation until text is allowed', () => {
    attach()
    vi.advanceTimersByTime(ARM_QUIET_MS)
    const held = new InputEvent('beforeinput', { bubbles: true, cancelable: true })
    field.dispatchEvent(held)
    expect(held.defaultPrevented).toBe(true)
    expect(reached).toEqual([])

    field.dispatchEvent(new MouseEvent('pointerdown', { button: 0, bubbles: true, cancelable: true }))
    expect(guard.textAllowed()).toBe(true)
    const allowed = new InputEvent('beforeinput', { bubbles: true, cancelable: true })
    field.dispatchEvent(allowed)
    expect(allowed.defaultPrevented).toBe(false)
  })

  it("a dialog's Tab moves focus as it unlocks, and an editor's only unlocks", () => {
    attach(true)
    vi.advanceTimersByTime(ARM_QUIET_MS)
    expect(press('Tab').defaultPrevented).toBe(false)
    expect(reached).toEqual(['Tab'])
    guard.detach()

    reached = []
    attach(false)
    vi.advanceTimersByTime(ARM_QUIET_MS)
    expect(press('Tab').defaultPrevented).toBe(true)
    expect(reached).toEqual([])
    expect(guard.textAllowed()).toBe(true)
    expect(press('Tab').defaultPrevented).toBe(false)
    expect(reached).toEqual(['Tab'])
  })

  it('leaves nothing behind once detached', () => {
    attach()
    guard.detach()
    expect(press('Enter').defaultPrevented).toBe(false)
    expect(reached).toEqual(['Enter'])
    vi.advanceTimersByTime(ARM_QUIET_MS)
    expect(onArmedChange).not.toHaveBeenCalled()
    // `afterEach` detaches again, which must stay harmless.
  })
})
