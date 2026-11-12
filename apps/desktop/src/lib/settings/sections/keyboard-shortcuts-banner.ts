/**
 * Pure conflict-banner logic for the Keyboard shortcuts section.
 *
 * When a captured combo conflicts, the banner's shape depends on WHAT it
 * conflicts with:
 *
 * - A conflict set that includes a command the app menu owns (`ownedByAppMenu`, macOS only) is
 *   unusable no matter what the user picks: AppKit owns the combo, so it can't
 *   reach Cmdr. The banner says so and offers only Cancel — no "Remove from
 *   other" (a lie: removing Cmdr's binding doesn't free the OS accelerator) and
 *   no "Keep both" (also a lie: the user's binding would never fire). The native
 *   command wins even in a MIXED set (native + a normal command), because the
 *   combo stays unusable regardless of the normal one.
 * - A purely non-native conflict keeps the existing resolvable banner
 *   (Remove-from-other / Keep-both / Cancel).
 */
import { ownedByAppMenu } from '#lib/commands/command-registry.js'
import type { Command, CommandScope } from '#lib/commands/types.js'
import type { BarTitleMnemonic } from '#lib/ipc/bindings.js'
import { getPlatform } from '#lib/platform/index.js'
import { formatConjunctionList } from '#lib/intl/list-format.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { systemReservation, type SystemReservation } from './system-reserved-keys'

/** A native conflict: the combo is reserved by macOS and can't reach Cmdr. */
export interface NativeConflict {
  kind: 'native'
  /** The native command that owns the combo (drives the banner copy). */
  command: Command
}

/** A fixed-key conflict: the combo is hardcoded in a component and always fires. */
export interface FixedConflict {
  kind: 'fixed'
  /** The fixed-key command that owns the combo (drives the banner copy). */
  command: Command
}

/** A normal, resolvable conflict between two in-app commands. */
export interface NormalConflict {
  kind: 'normal'
  /** The other command the combo is already bound to. */
  command: Command
}

/** A refusal, not a clash: typing takes the key first (`typingRefusal`), so a binding would never fire. */
export type TypingConflict = { kind: 'shiftLetter' } | { kind: 'bareKey' }

export type ConflictKind =
  | NativeConflict
  | FixedConflict
  | SystemConflict
  | MenuBarMnemonicConflict
  | NormalConflict
  | TypingConflict

/**
 * Classify a non-empty conflict set. A native command anywhere in the set makes
 * the whole combo unusable, so it wins; otherwise the first conflicting command
 * drives the resolvable banner (matching the prior single-conflict behavior).
 */
export function classifyConflict(conflicts: Command[]): ConflictKind | null {
  if (conflicts.length === 0) return null
  const native = conflicts.find(ownedByAppMenu)
  if (native) return { kind: 'native', command: native }
  // A fixed-key command's binding can't be removed ("Remove from other" would be
  // refused by the store) and always keeps firing ("Keep both" would race it), so
  // it makes the combo non-resolvable, second only to a native conflict.
  const fixed = conflicts.find((c) => c.fixedKey)
  if (fixed) return { kind: 'fixed', command: fixed }
  return { kind: 'normal', command: conflicts[0] }
}

/**
 * The honest banner copy for a native conflict, like
 * `⌘H is reserved by macOS (Hide Cmdr) and won't reach Cmdr. Pick a different combo.`
 * `combo` is shown in the current platform's display form (what the user pressed).
 */
export function reservedByMacOsMessage(combo: string, nativeCommand: Command): string {
  return tString('shortcuts.conflict.reservedByMacOs', { combo, command: nativeCommand.name })
}

/**
 * The honest banner copy for a fixed-key conflict, like
 * `↑ is a fixed key in Cmdr (Select previous file). Pick a different combo.`
 */
export function fixedKeyMessage(combo: string, fixedCommand: Command): string {
  return tString('shortcuts.conflict.fixedKey', { combo, command: fixedCommand.name })
}

/** The banner copy for a combo typing takes first, like `Sorry, ⇧A is how you type a capital letter, …`. */
export function typingConflictMessage(combo: string, kind: TypingConflict['kind']): string {
  return tString(kind === 'shiftLetter' ? 'shortcuts.conflict.shiftLetter' : 'shortcuts.conflict.bareKey', { combo })
}

/** A system conflict: the OS (or, on Linux, a desktop) usually takes the combo before Cmdr sees it. */
export interface SystemConflict {
  kind: 'system'
  /** Who takes it (drives the banner copy). */
  reservation: SystemReservation
}

/**
 * Classify a combo against this OS's reserved-key table (`system-reserved-keys.ts`). Checked
 * only when there's no in-app conflict (those banners take priority); a match produces a soft
 * warning with "Use anyway", not a refusal, since the user can free most of these.
 */
export function classifySystemShortcut(combo: string): SystemConflict | null {
  const reservation = systemReservation(combo)
  return reservation ? { kind: 'system', reservation } : null
}

/**
 * The banner copy for a system-owned combo, like
 * `⌘Space is usually taken by macOS (Spotlight), so it may never reach Cmdr. …`, or on Linux
 * `On KDE Plasma and Xfce, the desktop usually takes Ctrl+F3 for itself, …`. Each owner has its
 * own sentence: what's reserved, who reserves it, and where to free it differ per OS.
 */
export function systemShortcutMessage(combo: string, reservation: SystemReservation): string {
  switch (reservation.os) {
    case 'macos':
      return tString('shortcuts.conflict.systemShortcut', { combo, label: tString(reservation.featureKey) })
    case 'windows':
      return tString('shortcuts.conflict.takenByWindows', { combo, label: tString(reservation.featureKey) })
    case 'linux':
      // Proper nouns, joined the UI language's way (`formatConjunctionList`, never a `', '` join).
      return tString('shortcuts.conflict.takenByLinuxDesktops', {
        combo,
        desktops: formatConjunctionList(reservation.desktops),
      })
  }
}

/** A menu bar title's mnemonic: on Linux GTK opens `menu` on this `Alt+<letter>` before Cmdr sees it. */
export interface MenuBarMnemonicConflict {
  kind: 'menuBarMnemonic'
  /** The menu's title in the UI language (drives the banner copy). */
  menu: string
}

/**
 * The scopes whose keys only run while a modal surface holds the window's mnemonics off (the main
 * window's dialog gate: a dialog, an explorer overlay, the palette), or in a window whose bar has
 * none (the viewer's). Anything else wears a live bar: the panes, the Ask Cmdr rail, the operation
 * queue and About windows.
 */
const MNEMONICS_OFF_SCOPES: ReadonlySet<CommandScope> = new Set<CommandScope>([
  'Main window/Search dialog',
  'Main window/Multi-rename',
  'Main window/Operation log',
  'Main window/Volume chooser',
  'Main window/Favorites menu',
  'Command palette',
  'Onboarding',
  'Viewer window',
])

/**
 * On Linux, a recorded `Alt+<letter>` that a bar title underlines in the UI language
 * (`mnemonics`, from `getMenuBarTitleMnemonics`) opens that menu instead, for a command whose
 * scope wears a live bar. Soft, like the other reserved-key warnings: the key still works while a
 * dialog is up.
 */
export function classifyMenuBarMnemonic(
  combo: string,
  scope: CommandScope,
  mnemonics: readonly BarTitleMnemonic[],
): MenuBarMnemonicConflict | null {
  if (getPlatform() !== 'linux' || MNEMONICS_OFF_SCOPES.has(scope)) return null
  const letter = /^Alt\+(.)$/u.exec(combo)?.[1]?.toLowerCase()
  if (letter === undefined) return null
  const mnemonic = mnemonics.find((candidate) => candidate.letter === letter)
  return mnemonic ? { kind: 'menuBarMnemonic', menu: mnemonic.menu } : null
}

/** The banner copy for a bar-title mnemonic, like `Alt+F opens the File menu in Cmdr’s menu bar, …`. */
export function menuBarMnemonicMessage(combo: string, menu: string): string {
  return tString('shortcuts.conflict.opensMenuBarMenu', { combo, menu })
}
