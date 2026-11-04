/**
 * What a menu pick does in the page, for the bar and the right-click menus alike: the engine
 * answered WHAT the native item sends (`menu-engine.ts`'s `MenuClick`), and this sends it the way
 * the desktop's `handle_menu_event` does.
 */
import type { Payload } from './answer-types.js'
import type { EventTarget } from './backend.js'
import type { MenuClick } from './menu-engine.js'
import type { WindowHost } from './windows.js'

export interface MenuClickDeps {
  emit: (event: string, payload: unknown, target: EventTarget) => void
  /** An app command, as `handle_menu_event` calls one directly (Add to favorites). */
  invoke: (cmd: string, payload: Payload) => Promise<unknown>
  windows: Pick<WindowHost, 'front' | 'close'>
}

/** Sends what the pick asked for, after focus went back where it was, as a native pick does. */
export function performMenuClick(outcome: MenuClick, deps: MenuClickDeps): void {
  switch (outcome.kind) {
    case 'emit':
      deps.emit(outcome.event, outcome.payload, { kind: 'label', label: outcome.to })
      return
    case 'invoke':
      // Fire and forget, as the desktop spawns it: a refusal is the command's own to log.
      deps.invoke(outcome.command, outcome.args).catch((refusal: unknown) => {
        // eslint-disable-next-line no-console -- the demo's developer reads the devtools console
        console.warn(`[demo] ${outcome.command} from a menu was refused`, refusal)
      })
      return
    case 'nativeEdit': {
      // The responder chain's stand-in: the edit goes to the window in front, which has the text focus.
      const front = deps.windows.front()
      if (!front) return
      front.frame.focus()
      // eslint-disable-next-line @typescript-eslint/no-deprecated -- the one way a page can hand an edit to the focused field, which is what the native selector does; browsers keep it for exactly this
      front.frame.contentDocument?.execCommand(outcome.action)
      return
    }
    case 'closeFocusedWindow': {
      const front = deps.windows.front()
      if (front) deps.windows.close(front.label)
      return
    }
    case 'nothing':
      return
  }
}
