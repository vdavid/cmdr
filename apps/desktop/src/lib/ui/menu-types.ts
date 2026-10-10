import type { IconName } from './icons/icon-map'

/**
 * The vocabulary of the house `Menu` (`lib/ui/Menu.svelte`). Lives in a `.ts` (not the
 * component's module script like `SelectItem`) because non-Svelte glue — menu controllers
 * such as `file-explorer/pane/enter-menu.ts` — consumes it, and a type imported from a
 * `.svelte` file resolves to `any` under the plain-TypeScript lint service.
 *
 * `T` is the caller's own payload, carried on `MenuItem.data` and handed back untouched on
 * select and to every row snippet, so a consumer never has to look its row up again. It
 * defaults to `unknown`, so `createMenu({ getSections, onSelect })` needs no type argument;
 * a caller that wants typed `data` writes `createMenu<VolumeInfo>({ … })` and gets it in
 * `onSelect` and in every snippet's `MenuRowContext`.
 */

/** A lucide glyph, or an image the caller already has a URL for (a volume or folder icon). */
export type MenuIcon = { lucide: IconName } | { src: string }

/**
 * Why a row carries a checkmark, which decides what a screen reader hears:
 *
 * - `toggle`: an on/off switch (a per-share setting). The row is a `menuitemcheckbox` with
 *   `aria-checked`, ON OR OFF: an unchecked toggle still says "unchecked". That's why `checked`
 *   lives inside, so an off toggle and a plain action can't be the same shape.
 * - `current`: this row is where you are (the volume the pane is on). The row stays a plain
 *   `menuitem`, drawn checked, with `aria-current="location"`.
 *
 * ❗ Not a `menuitemradio` for `current`: a checkbox or radio menu item's children are
 * presentational, so the eject button a volume row hosts would vanish for VoiceOver (axe's
 * `nested-interactive`). For the same reason a `toggle` row must not host its own controls.
 */
export type MenuItemCheck = { kind: 'toggle'; checked: boolean } | { kind: 'current' }

/** One row. `value` is the stable identity: it's what `onSelect` emits and what the highlight tracks. */
export interface MenuItem<T = unknown> {
  value: string
  label: string
  icon?: MenuIcon
  /**
   * A toggle or the "you are here" row (see `MenuItemCheck`), on a submenu row too. A checked
   * toggle and a current row render the leading checkmark; the checkmark column is always
   * reserved, so rows stay aligned. Picking a toggle only activates it: the consumer flips its
   * own state.
   */
  check?: MenuItemCheck
  /**
   * A single character shown in the leftmost column, which also activates the row when typed
   * (digits only today). The column appears only in a menu where at least one row declares one,
   * and the rows that don't get a blank placeholder so every label still lines up. Top-level
   * rows only: a submenu row's is ignored.
   */
  accelerator?: string
  /** Optional letter shortcut, rendered at the right edge and activated without modifiers. */
  shortcut?: string
  /** Greyed, skipped by the keyboard and the pointer's cursor, never activates. Submenu rows too. */
  disabled?: boolean
  tooltip?: string
  /**
   * One level only: a submenu item's own `submenu` is ignored. A right-click on the row opens
   * it too, so both doors show the same rows.
   */
  submenu?: MenuItem<T>[]
  /**
   * Picking this row leaves the menu open, closing only its submenu: an eject (so several
   * drives can go in a row) or a rename (which happens in the row itself).
   */
  keepsMenuOpen?: boolean
  /** Draws a rule above this row. Submenu rows only: a top-level list splits into sections. */
  separatorBefore?: boolean
  data?: T
}

export interface MenuSection<T = unknown> {
  id: string
  heading?: string
  items: MenuItem<T>[]
  /** Rows reorder within this section by drag and ⌥↑/⌥↓; the caller persists in `onReorder`. */
  reorderable?: boolean
  /** Shown (disabled, unfocusable) when the section is empty, so the section still reads as a real state. */
  emptyLabel?: string
  /**
   * Folds the section behind a disclosure row, which takes the place of `heading`. The CALLER
   * owns `expanded` (and persists it, if it should outlive the menu): a flip comes back through
   * `onDisclosureChange`, and the menu shows whatever the next `getSections()` says.
   */
  disclosure?: MenuDisclosure
}

/** A section's disclosure row: a full row the cursor lands on, that shows or hides the rows under it. */
export interface MenuDisclosure {
  expanded: boolean
  label: string
  icon?: MenuIcon
  tooltip?: string
}

/** What `onDisclosureChange` receives: the section, and the state the user asked for. */
export interface MenuDisclosureChange {
  sectionId: string
  expanded: boolean
}

/** The one argument every row snippet (`label`, `trailing`, `below`) takes. */
export interface MenuRowContext<T = unknown> {
  item: MenuItem<T>
  section: MenuSection<T>
  /** The row's index WITHIN its section, which is also what a reorder moves. */
  index: number
  highlighted: boolean
  dragging: boolean
}

/** What `onReorder` receives, once, on drop (or on a ⌥↑/⌥↓ that actually moves something). */
export interface MenuReorder {
  sectionId: string
  /** The section's item values in their new order: hand this straight to a persist call. */
  orderedValues: string[]
  from: number
  to: number
}

/** Where an open menu pins itself: under an element, or at a viewport point (a context menu). */
export type MenuAnchor = { kind: 'element'; element: HTMLElement } | { kind: 'point'; x: number; y: number }

/**
 * How a row got activated, handed to `onSelect` beside the item.
 *
 * The primitive is the only thing that knows: by the time a consumer sees the pick, the
 * click, the Enter, and the digit have all collapsed into the same call. A consumer that
 * cares (the favorites menu, whose analytics ask whether the number column earns its place)
 * would otherwise have to sniff `onKey` and rebuild the answer, which drifts the moment the
 * keyboard contract grows a case.
 */
export type MenuActivationSource =
  /** A click, or a drag that never crossed the threshold. */
  | 'pointer'
  /** Enter or Space on the highlighted row. */
  | 'keyboard'
  /** The row's `accelerator` character was typed. */
  | 'accelerator'
  /** The row's right-aligned letter shortcut was typed. */
  | 'shortcut'
