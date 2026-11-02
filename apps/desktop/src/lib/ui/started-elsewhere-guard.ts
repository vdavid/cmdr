/**
 * Keeps a stray keystroke from touching a dialog or editor someone else started.
 *
 * An agent can open a dialog while a person is typing in the rail, and `ModalDialog` takes focus as
 * it appears, so the Enter ending their sentence would confirm a copy they never read. So what a
 * guarded caller opens starts unarmed. It arms once nobody has touched a key for `ARM_QUIET_MS` and
 * no key is down. Until then it swallows every key but Escape, and for the first `PRESS_IGNORE_MS` it
 * ignores primary-button presses. Arming alone doesn't let text in: someone who pauses and carries on
 * typing would edit the destination, so text waits for a click or a Tab too.
 *
 * It only ever filters input. `dialog_confirm` doesn't press anything, so it passes through.
 *
 * `createGuardMachine` is the pure half, driven by whatever clock the caller hands it, and
 * `attachStartedElsewhereGuard` is the capture-phase listener over it. Capture on `window`, so it runs
 * ahead of the dialog's own key handling, the focus trap, and the field. Rationale:
 * `lib/ui/DETAILS.md` § The started-elsewhere guard.
 */

import type { Initiator } from '#lib/ipc/bindings.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('startedElsewhereGuard')

/** How long the keyboard has to stay quiet before a guarded dialog or editor arms. */
export const ARM_QUIET_MS = 800

/** How long after it opens a primary-button press still counts as stray. */
export const PRESS_IGNORE_MS = 300

/** Who a guarded dialog or editor says started it. Only a tool's caller is ever guarded. */
export type ToolInitiator = Extract<Initiator, 'agent' | 'aiClient'>

/**
 * What a dialog's `startedBy` prop gets: who to name when the opening event said `guard`, else nobody,
 * which leaves the dialog unguarded. Anything but the agent reads as an AI client, so a `guard` can
 * never come out as a person's.
 */
export function startedByWhenGuarded(initiator: Initiator, guard: boolean | undefined): ToolInitiator | undefined {
  if (guard !== true) return undefined
  return initiator === 'agent' ? 'agent' : 'aiClient'
}

/** The line a guarded dialog or editor shows, and a screen reader hears when it changes. */
export function startedElsewhereLine(startedBy: ToolInitiator, armed: boolean): string {
  const who = tString(startedBy === 'agent' ? 'ui.startedElsewhere.agent' : 'ui.startedElsewhere.aiClient')
  return armed ? who : `${who} ${tString('ui.startedElsewhere.keysWait')}`
}

/** What a key event carries that the machine reads. */
export interface GuardKey {
  /** Which physical key, so a held key can be told from a new one. */
  id: string
  key: string
  repeat: boolean
  /** The modifiers the event says are down, which may include ones pressed before the guard existed. */
  modifiers: readonly GuardModifier[]
}

export type GuardModifier = 'Shift' | 'Control' | 'Alt' | 'Meta'

const MODIFIERS: readonly GuardModifier[] = ['Shift', 'Control', 'Alt', 'Meta']

/** macOS sends one keydown as Caps Lock turns on and one keyup as it turns off, so it's "down" for hours. */
const LOCKS: readonly string[] = ['CapsLock', 'NumLock', 'ScrollLock']

/**
 * What to do with an event. `unlocked` is the first armed Tab: it's the gesture that lets text in, and
 * whether it also moves focus is the host's call.
 */
export type GuardVerdict = 'pass' | 'swallow' | 'unlocked'

function isModifier(key: string): key is GuardModifier {
  return (MODIFIERS as readonly string[]).includes(key)
}

/** The keys that mean something on a dialog or editor before anyone has clicked or tabbed into it. */
function answersWithoutAGesture(key: string): boolean {
  return key === 'Enter' || key === 'Escape' || isModifier(key)
}

export function createGuardMachine(openedAt: number) {
  const down = new Set<string>()
  let lastKeyAt = openedAt
  let armed = false
  let gestured = false

  /**
   * A modifier that went down before this guard existed only shows in later events' state, and a key
   * that sends no repeats gives no other sign. Also heals the other way: macOS drops a keyup now and
   * then, so a modifier the event says is up is up.
   */
  function reconcileModifiers(modifiers: readonly GuardModifier[]): boolean {
    let unseen = false
    for (const modifier of MODIFIERS) {
      const isDown = modifiers.includes(modifier)
      if (isDown && !down.has(modifier)) {
        down.add(modifier)
        unseen = true
      } else if (!isDown) {
        down.delete(modifier)
      }
    }
    return unseen
  }

  return {
    get armed(): boolean {
      return armed
    },
    /** Whether a click or a Tab has happened, which lets focus reach a field. */
    get gestured(): boolean {
      return gestured
    },
    /** Whether text may land in a field: armed, and after a click or a Tab. */
    get textAllowed(): boolean {
      return armed && gestured
    },

    keyDown(event: GuardKey, now: number): GuardVerdict {
      lastKeyAt = now
      const heldSinceBefore = reconcileModifiers(event.modifiers) && !isModifier(event.key)
      const heldUnseen = event.repeat && !down.has(event.id)
      if (isModifier(event.key)) down.add(event.key)
      else if (!LOCKS.includes(event.key)) down.add(event.id)

      if (event.key === 'Escape') return 'pass'
      if (armed && (heldSinceBefore || heldUnseen)) armed = false
      if (!armed) return 'swallow'
      if (event.key === 'Tab' && !gestured) {
        gestured = true
        return 'unlocked'
      }
      if (!gestured && !answersWithoutAGesture(event.key)) return 'swallow'
      return 'pass'
    },

    keyUp(event: GuardKey, now: number): GuardVerdict {
      lastKeyAt = now
      reconcileModifiers(event.modifiers)
      down.delete(isModifier(event.key) ? event.key : event.id)
      // macOS sends no keyup for a key released while Command is down, and it'd stay down forever.
      if (event.key === 'Meta') for (const id of down) if (!isModifier(id)) down.delete(id)
      return event.key === 'Escape' || armed ? 'pass' : 'swallow'
    },

    /** Anything else that means someone's at the keyboard: an IME composing, say. */
    activity(now: number): void {
      lastKeyAt = now
    },

    /** The window lost focus, so whatever's down now, its keyup won't reach us. */
    windowLeft(now: number): void {
      down.clear()
      lastKeyAt = now
    },

    press(now: number): GuardVerdict {
      if (now - openedAt < PRESS_IGNORE_MS) return 'swallow'
      gestured = true
      return 'pass'
    },

    /** Arms if the time has come. Answers whether that changed anything. */
    tick(now: number): boolean {
      if (armed || down.size > 0 || now - lastKeyAt < ARM_QUIET_MS) return false
      armed = true
      return true
    },

    /** How long until `tick` could arm, for the host's timer. */
    quietLeft(now: number): number {
      return Math.max(0, lastKeyAt + ARM_QUIET_MS - now)
    },
  }
}

export interface StartedElsewhereGuardOptions {
  /** Where a primary-button press counts: the dialog's overlay, or the editor's field. */
  scope: () => Element | null | undefined
  /**
   * Where focus waits until a click or Tab, for a dialog. Anything that focuses a field on its own,
   * a child's autofocus or the first-field fallback, comes back here.
   */
  parkFocus?: () => HTMLElement | null | undefined
  /** Whether the Tab that lets text in also moves focus, as a dialog's does. An editor's stays put. */
  tabMovesFocus: boolean
  onArmedChange: (armed: boolean) => void
}

export interface StartedElsewhereGuard {
  detach: () => void
  /** Whether text may reach a field yet. */
  textAllowed: () => boolean
}

function keyOf(event: KeyboardEvent): GuardKey {
  const modifiers = MODIFIERS.filter((modifier) => event.getModifierState(modifier))
  return { id: event.code || event.key, key: event.key, repeat: event.repeat, modifiers }
}

function swallow(event: Event): void {
  event.preventDefault()
  event.stopImmediatePropagation()
}

/** Starts guarding now. Every listener and timer is gone after `detach`. */
export function attachStartedElsewhereGuard(options: StartedElsewhereGuardOptions): StartedElsewhereGuard {
  const machine = createGuardMachine(Date.now())
  let timer: ReturnType<typeof setTimeout> | null = null
  // The click that ends a swallowed press would otherwise still land.
  let swallowClick = false

  function schedule(): void {
    if (timer !== null) clearTimeout(timer)
    timer = null
    if (!machine.armed) timer = setTimeout(check, Math.max(machine.quietLeft(Date.now()), 1))
  }

  function report(wasArmed: boolean): void {
    if (machine.armed === wasArmed) return
    log.debug('{state}', { state: machine.armed ? 'armed' : 'disarmed' })
    options.onArmedChange(machine.armed)
  }

  function check(): void {
    timer = null
    const wasArmed = machine.armed
    machine.tick(Date.now())
    report(wasArmed)
    // Still unarmed: a key is down, and its keyup will schedule the next look.
    if (!machine.armed && machine.quietLeft(Date.now()) > 0) schedule()
  }

  function onKeyDown(event: KeyboardEvent): void {
    const wasArmed = machine.armed
    const verdict = machine.keyDown(keyOf(event), Date.now())
    report(wasArmed)
    schedule()
    // Armed, it only guards its own field or overlay: an editor's extension question, up over it, keeps its keys.
    if (machine.armed && !inScope(event.target)) return
    if (verdict === 'swallow' || (verdict === 'unlocked' && !options.tabMovesFocus)) swallow(event)
  }

  function onKeyUp(event: KeyboardEvent): void {
    const verdict = machine.keyUp(keyOf(event), Date.now())
    schedule()
    if (verdict === 'swallow') swallow(event)
  }

  function inScope(target: EventTarget | null): boolean {
    return target instanceof Node && options.scope()?.contains(target) === true
  }

  function onPointerDown(event: PointerEvent | MouseEvent): void {
    if (event.button !== 0 || !inScope(event.target)) return
    if (machine.press(Date.now()) !== 'swallow') return
    swallowClick = true
    swallow(event)
  }

  function onClick(event: MouseEvent): void {
    if (!swallowClick) return
    swallowClick = false
    if (inScope(event.target)) swallow(event)
  }

  /** Paste and dictation arrive here with no keydown to swallow. */
  function onBeforeInput(event: Event): void {
    if (!machine.textAllowed && inScope(event.target)) swallow(event)
  }

  function onComposition(): void {
    machine.activity(Date.now())
    schedule()
  }

  function onWindowBlur(): void {
    machine.windowLeft(Date.now())
    schedule()
  }

  function onFocusIn(event: FocusEvent): void {
    if (machine.gestured) return
    const park = options.parkFocus?.()
    if (park && event.target !== park && inScope(event.target)) park.focus({ preventScroll: true })
  }

  window.addEventListener('keydown', onKeyDown, true)
  window.addEventListener('keyup', onKeyUp, true)
  window.addEventListener('pointerdown', onPointerDown, true)
  window.addEventListener('mousedown', onPointerDown, true)
  window.addEventListener('click', onClick, true)
  window.addEventListener('beforeinput', onBeforeInput, true)
  window.addEventListener('compositionstart', onComposition, true)
  window.addEventListener('compositionupdate', onComposition, true)
  window.addEventListener('blur', onWindowBlur)
  window.addEventListener('focusin', onFocusIn, true)
  schedule()

  return {
    detach() {
      if (timer !== null) clearTimeout(timer)
      timer = null
      window.removeEventListener('keydown', onKeyDown, true)
      window.removeEventListener('keyup', onKeyUp, true)
      window.removeEventListener('pointerdown', onPointerDown, true)
      window.removeEventListener('mousedown', onPointerDown, true)
      window.removeEventListener('click', onClick, true)
      window.removeEventListener('beforeinput', onBeforeInput, true)
      window.removeEventListener('compositionstart', onComposition, true)
      window.removeEventListener('compositionupdate', onComposition, true)
      window.removeEventListener('blur', onWindowBlur)
      window.removeEventListener('focusin', onFocusIn, true)
    },
    textAllowed: () => machine.textAllowed,
  }
}
