/**
 * Test-only keydown builders for what a synthetic `KeyboardEvent` can't say on its own.
 */
import { isMacOS } from '#lib/platform/index.js'

/**
 * The event flag of the platform's primary modifier, as of the call: `metaKey` (⌘) on macOS,
 * `ctrlKey` on Windows and Linux. For a test that presses a `cmdOrCtrl` key on whatever
 * platform it runs on.
 */
export function primaryModifierInit(): Pick<KeyboardEventInit, 'metaKey' | 'ctrlKey'> {
  return isMacOS() ? { metaKey: true } : { ctrlKey: true }
}

/**
 * A keydown with AltGr held: `getModifierState('AltGraph')` answers true, the way a
 * browser reports the right Alt key on an AltGr layout. Pass the flags that platform's
 * webview sets with it: Windows adds `ctrlKey` and `altKey`, Linux sets neither.
 */
export function keydownWithAltGraph(init: KeyboardEventInit): KeyboardEvent {
  const event = new KeyboardEvent('keydown', init)
  const base = event.getModifierState.bind(event)
  Object.defineProperty(event, 'getModifierState', {
    value: (key: string) => key === 'AltGraph' || base(key),
  })
  return event
}
