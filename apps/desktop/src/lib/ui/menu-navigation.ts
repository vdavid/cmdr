/**
 * Pure keyboard math for the house `Menu` (`lib/ui/Menu.svelte`): which rows the cursor
 * can land on, where it goes next, and what a keystroke means. No DOM and no state, so
 * the whole keyboard contract is unit-testable; `menu-controller.svelte.ts` applies the
 * answers.
 */

import type { MenuItem, MenuSection } from './menu-types'

/** What a keystroke means to an open menu. `none` is "not ours"; the controller still swallows it. */
export type MenuAction =
  | { kind: 'move'; delta: -1 | 1 }
  | { kind: 'edge'; edge: 'first' | 'last' }
  | { kind: 'activate' }
  | { kind: 'close' }
  | { kind: 'openSubmenu' }
  | { kind: 'closeSubmenu' }
  /** Move the cursor WITHIN an open submenu, which has its own. */
  | { kind: 'moveSubmenu'; delta: -1 | 1 }
  /** Consumed on purpose so it can't reach the parent menu (ArrowRight inside a submenu). */
  | { kind: 'absorb' }
  | { kind: 'reorder'; delta: -1 | 1 }
  /**
   * A digit was typed. `char` is the character it stands for; `itemByAccelerator` says which row
   * (if any) claims it. Unclaimed is still an accelerator, not a `none`: an open menu owns the
   * keyboard, so the digit is swallowed either way.
   */
  | { kind: 'accelerator'; char: string }
  /** Open or fold the section whose disclosure row the cursor is on. */
  | { kind: 'disclose'; expanded: boolean }
  /** Back from a row inside an open section to that section's disclosure row. */
  | { kind: 'toDisclosure' }
  | { kind: 'none' }

export interface MenuKeyContext {
  /** The highlighted row has a submenu, so ArrowRight opens it. */
  hasSubmenu: boolean
  /**
   * Where the submenu stands. `shown` is open with no cursor of its own (a hover opened it), so
   * the parent list still owns the arrows; `entered` has its own cursor (`→`, or the pointer
   * reached in), so the arrows walk its rows.
   */
  submenu: 'closed' | 'shown' | 'entered'
  /** The highlighted row sits in a `reorderable` section, so ⌥↑/⌥↓ move it. */
  reorderable: boolean
  /** The highlighted row IS a section's disclosure row, open or folded. */
  disclosure?: 'expanded' | 'collapsed'
  /** The highlighted row sits inside a section with a disclosure row, so ← goes back to it. */
  insideDisclosure?: boolean
}

/**
 * The value a section's disclosure row goes by: what the cursor holds, what `data-menu-row`
 * carries, and what the surface hands back on a click. A caller's own values must not start
 * with `menu-disclosure:`.
 */
export function disclosureRowValue(sectionId: string): string {
  return `menu-disclosure:${sectionId}`
}

/** The section whose disclosure row `value` is, or null for any other row. */
export function disclosureOf<T>(sections: readonly MenuSection<T>[], value: string): MenuSection<T> | null {
  return sections.find((section) => section.disclosure && disclosureRowValue(section.id) === value) ?? null
}

/** A section whose rows are folded away: not rendered, not walked, and claiming no key. */
function isFolded<T>(section: MenuSection<T>): boolean {
  return section.disclosure?.expanded === false
}

/**
 * Every row the cursor may land on, in display order: headings, separators, empty
 * placeholders, and disabled rows are not among them. A disclosure row is, and the rows it
 * folds away are not.
 */
export function navigableValues<T>(sections: readonly MenuSection<T>[]): string[] {
  const values: string[] = []
  for (const section of sections) {
    if (section.disclosure) values.push(disclosureRowValue(section.id))
    if (isFolded(section)) continue
    for (const item of section.items) {
      if (!item.disabled) values.push(item.value)
    }
  }
  return values
}

/**
 * The value `delta` steps from `current`, wrapping at both ends. With nothing highlighted,
 * a downward step starts at the first row and an upward one at the last. `null` when there
 * is nothing to land on.
 */
export function nextValue(values: readonly string[], current: string | null, delta: number): string | null {
  if (values.length === 0) return null
  const index = current === null ? -1 : values.indexOf(current)
  if (index < 0) return delta > 0 ? values[0] : values[values.length - 1]
  const next = (index + delta + values.length) % values.length
  return values[next]
}

/** The section a row belongs to, plus its index WITHIN that section (disabled rows counted). */
export function sectionOf<T>(
  sections: readonly MenuSection<T>[],
  value: string,
): { section: MenuSection<T>; index: number } | null {
  for (const section of sections) {
    const index = section.items.findIndex((item) => item.value === value)
    if (index >= 0) return { section, index }
  }
  return null
}

/** The item for a value, looking one level into submenus so a submenu pick resolves too. */
export function itemOf<T>(sections: readonly MenuSection<T>[], value: string): MenuItem<T> | null {
  for (const section of sections) {
    for (const item of section.items) {
      if (item.value === value) return item
      const nested = item.submenu?.find((child) => child.value === value)
      if (nested) return nested
    }
  }
  return null
}

/**
 * The enabled row that claims an accelerator character, across every section. A DISABLED row
 * claims nothing: its accelerator must not activate it, and nothing else may steal the key
 * either, so the answer is simply "no row" and the controller does nothing with it.
 */
export function itemByAccelerator<T>(sections: readonly MenuSection<T>[], char: string): MenuItem<T> | null {
  for (const section of sections) {
    // A folded row is out of sight, so a key must not open it either.
    if (isFolded(section)) continue
    for (const item of section.items) {
      if ((item.accelerator === char || item.shortcut === char) && !item.disabled) return item
    }
  }
  return null
}

/**
 * The menu key's digit or letter, or null for anything else. Digits match on `event.code` so
 * the physical key decides: a layout where digits need Shift (AZERTY) still types `1`, and the
 * numpad counts as the same key. Letters follow `event.key` to match the user's layout.
 *
 * ❗ This is a CLASS-OF-KEY matcher (any digit key, no required modifier), which is exactly what
 * `cmdr/no-raw-key-match` exists to allow through: there's no modifier left unconstrained, and
 * the rule's target — a hand-rolled single combo that should have been `eventMatchesCommand` —
 * has no accelerator equivalent, since the caller's data decides which digits and letters exist.
 */
export function acceleratorChar(event: KeyboardEvent): string | null {
  const digit = /^(?:Digit|Numpad)(\d)$/.exec(event.code)?.[1]
  if (digit) return digit
  return /^[a-z]$/i.test(event.key) ? event.key.toUpperCase() : null
}

/**
 * True when the event carries a modifier that makes it somebody else's combo. A bare cursor
 * key is the menu's; ⌘↓ / ⌃↓ / ⌥↓ mean other things and must pass through untouched. Shift is
 * deliberately not in here: ⇧Enter is still Enter to a menu, and on AZERTY the digits need it.
 */
function hasCommandModifier(event: KeyboardEvent): boolean {
  return event.metaKey || event.ctrlKey || event.altKey
}

/** Exactly ⌥ and nothing else: ⌥⌘↑ and ⇧⌥↑ mean other things and must not reorder on their way. */
function isReorderCombo(event: KeyboardEvent): boolean {
  return event.altKey && !event.metaKey && !event.ctrlKey && !event.shiftKey
}

/** An entered submenu owns the cursor keys: it has its own cursor, and its own rows to walk. */
function submenuKeyAction(key: string): MenuAction {
  switch (key) {
    case 'ArrowLeft':
    case 'Escape':
      return { kind: 'closeSubmenu' }
    case 'Enter':
    case ' ':
      return { kind: 'activate' }
    case 'ArrowUp':
      return { kind: 'moveSubmenu', delta: -1 }
    case 'ArrowDown':
      return { kind: 'moveSubmenu', delta: 1 }
    case 'ArrowRight':
      // One level only, so there is nothing further right: swallow it rather than letting it
      // reach the parent list and move the cursor behind the open submenu.
      return { kind: 'absorb' }
    default:
      return { kind: 'none' }
  }
}

/** The plain list contract, with the cursor on a row. */
function rowKeyAction(key: string, hasSubmenu: boolean): MenuAction {
  switch (key) {
    case 'ArrowDown':
      return { kind: 'move', delta: 1 }
    case 'ArrowUp':
      return { kind: 'move', delta: -1 }
    case 'Home':
      return { kind: 'edge', edge: 'first' }
    case 'End':
      return { kind: 'edge', edge: 'last' }
    case 'Enter':
    case ' ':
      return { kind: 'activate' }
    case 'Escape':
      return { kind: 'close' }
    case 'ArrowRight':
      return hasSubmenu ? { kind: 'openSubmenu' } : { kind: 'none' }
    default:
      return { kind: 'none' }
  }
}

/**
 * The cursor on a disclosure row: → opens the section and ← folds it (each a no-op when it's
 * already that way, rather than wandering off), and Enter or Space flips it. Everything else is
 * the plain list contract.
 */
function disclosureKeyAction(key: string, state: 'expanded' | 'collapsed'): MenuAction {
  const expanded = state === 'expanded'
  switch (key) {
    case 'ArrowRight':
      return expanded ? { kind: 'none' } : { kind: 'disclose', expanded: true }
    case 'ArrowLeft':
      return expanded ? { kind: 'disclose', expanded: false } : { kind: 'none' }
    case 'Enter':
    case ' ':
      return { kind: 'disclose', expanded: !expanded }
    default:
      return rowKeyAction(key, false)
  }
}

/** The plain list contract on a closed-submenu row, plus ← back to its section's disclosure row. */
function closedRowKeyAction(key: string, context: MenuKeyContext): MenuAction {
  if (context.disclosure) return disclosureKeyAction(key, context.disclosure)
  if (key === 'ArrowLeft' && context.insideDisclosure) return { kind: 'toDisclosure' }
  return rowKeyAction(key, context.hasSubmenu)
}

/**
 * A submenu a hover opened, still cursorless: the parent list keeps the arrows, `→` enters the
 * submenu, and `←` / Escape close it.
 */
function shownSubmenuKeyAction(key: string): MenuAction {
  switch (key) {
    case 'ArrowLeft':
    case 'Escape':
      return { kind: 'closeSubmenu' }
    default:
      return rowKeyAction(key, true)
  }
}

/** What one keystroke means to an open menu, given what the cursor is sitting on. */
export function menuKeyAction(event: KeyboardEvent, context: MenuKeyContext): MenuAction {
  if (context.reorderable && isReorderCombo(event)) {
    if (event.key === 'ArrowUp') return { kind: 'reorder', delta: -1 }
    if (event.key === 'ArrowDown') return { kind: 'reorder', delta: 1 }
  }
  if (hasCommandModifier(event)) return { kind: 'none' }
  // Ahead of the list's own keys, and across every section: no arrow, Home/End, or submenu key
  // is a digit, so nothing competes, and an accelerator works with a submenu open too.
  const char = acceleratorChar(event)
  if (char !== null) return { kind: 'accelerator', char }
  switch (context.submenu) {
    case 'entered':
      return submenuKeyAction(event.key)
    case 'shown':
      return shownSubmenuKeyAction(event.key)
    case 'closed':
      return closedRowKeyAction(event.key, context)
  }
}
