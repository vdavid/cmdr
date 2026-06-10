/**
 * Pure conflict-banner logic for the Keyboard shortcuts section.
 *
 * When a captured combo conflicts, the banner's shape depends on WHAT it
 * conflicts with:
 *
 * - A conflict set that includes a macOS-native command (`nativeShortcut`) is
 *   unusable no matter what the user picks: AppKit owns the combo, so it can't
 *   reach Cmdr. The banner says so and offers only Cancel — no "Remove from
 *   other" (a lie: removing Cmdr's binding doesn't free the OS accelerator) and
 *   no "Keep both" (also a lie: the user's binding would never fire). The native
 *   command wins even in a MIXED set (native + a normal command), because the
 *   combo stays unusable regardless of the normal one.
 * - A purely non-native conflict keeps the existing resolvable banner
 *   (Remove-from-other / Keep-both / Cancel).
 */
import type { Command } from '$lib/commands/types'

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

export type ConflictKind = NativeConflict | FixedConflict | SystemConflict | NormalConflict

/**
 * Classify a non-empty conflict set. A native command anywhere in the set makes
 * the whole combo unusable, so it wins; otherwise the first conflicting command
 * drives the resolvable banner (matching the prior single-conflict behavior).
 */
export function classifyConflict(conflicts: Command[]): ConflictKind | null {
  if (conflicts.length === 0) return null
  const native = conflicts.find((c) => c.nativeShortcut)
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
  return `${combo} is reserved by macOS (${nativeCommand.name}) and won't reach Cmdr. Pick a different combo.`
}

/**
 * The honest banner copy for a fixed-key conflict, like
 * `↑ is a fixed key in Cmdr (Select previous file). Pick a different combo.`
 */
export function fixedKeyMessage(combo: string, fixedCommand: Command): string {
  return `${combo} is a fixed key in Cmdr (${fixedCommand.name}) and can't be reassigned. Pick a different combo.`
}

/** A system conflict: macOS itself usually owns the combo (Spotlight, Mission Control, …). */
export interface SystemConflict {
  kind: 'system'
  /** The macOS feature that owns the combo (drives the banner copy). */
  label: string
}

/**
 * Default macOS system-wide shortcuts (display form, as `formatKeyCombo`
 * produces them on macOS). Best-effort: users can disable or remap these in
 * System Settings, so the banner warns and offers "Use anyway" instead of
 * refusing. Keys only ever match on macOS — other platforms capture
 * `Ctrl+…`-style strings.
 */
const macSystemShortcutToFeatureMap: Record<string, string> = {
  '⌘Space': 'Spotlight',
  '⌥⌘Space': 'Finder search window',
  '⌃⌘Space': 'Character Viewer',
  '⌃Space': 'input source switching',
  '⌘Tab': 'the app switcher',
  '⌃↑': 'Mission Control',
  '⌃↓': 'App windows',
  '⌃←': 'Spaces',
  '⌃→': 'Spaces',
  '⌘⇧3': 'screenshots',
  '⌘⇧4': 'screenshots',
  '⌘⇧5': 'screen recording',
  '⌘⇧Q': 'logging out',
  '⌃⌘Q': 'locking the screen',
  '⌥⌘⎋': 'Force Quit',
}

/**
 * Classify a combo against the macOS system-shortcut list. Checked only when
 * there's no in-app conflict (those banners take priority); a match produces a
 * soft warning, not a refusal.
 */
export function classifySystemShortcut(combo: string): SystemConflict | null {
  const label = macSystemShortcutToFeatureMap[combo]
  return label ? { kind: 'system', label } : null
}

/**
 * The banner copy for a system-owned combo, like
 * `⌘Space is usually taken by macOS (Spotlight), so it may never reach Cmdr.`
 */
export function systemShortcutMessage(combo: string, label: string): string {
  return `${combo} is usually taken by macOS (${label}), so it may never reach Cmdr. You can free it up in System Settings > Keyboard.`
}
