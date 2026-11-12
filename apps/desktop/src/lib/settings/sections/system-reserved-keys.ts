/**
 * The keys each OS usually takes before Cmdr sees them, one table per platform, for the Settings
 * recorder's soft "usually taken" warning (`keyboard-shortcuts-banner.ts`) and the default keymap's
 * invariants (`lib/shortcuts/keymap-invariants.test.ts`).
 *
 * Keyed by the CANONICAL combo `formatKeyCombo` produces on that OS (⌘⌃⌥⇧ glyphs on macOS,
 * `Ctrl+Alt+Shift+Super` words elsewhere): Apple spells force quit `⌥⌘⎋` and Microsoft writes
 * `Ctrl+Alt+Del`, but a keypress arrives as `⌘⌥Escape` and `Ctrl+Alt+Delete`, and a table spelled
 * any other way silently never matches (`shortcut-vocabulary.test.ts` pins all three).
 *
 * Only entries verified from each OS's own source go in: `docs/notes/windows-port/keymap-decisions.md`
 * § "Windows checks" (Microsoft's shortcut list) and § "Reservations behind the Linux keys" (each
 * desktop's default bindings, read in source). Best-effort: a user can free any of these in the
 * OS's settings, so the banner warns and offers "Use anyway" rather than refusing.
 *
 * Cmdr can't yet ask which Linux desktop is running (task 59), so a Linux entry names every listed
 * desktop that takes the combo. Once it can, `systemReservation` narrows `desktops` to the running
 * one, and nothing else changes shape.
 */
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { getPlatform, type Platform } from '#lib/platform/index.js'

/** The Linux desktops whose default bindings the table reads, in the order a warning names them. */
export const LINUX_DESKTOPS = ['GNOME', 'KDE Plasma', 'Xfce', 'Cinnamon', 'Ubuntu'] as const
export type LinuxDesktop = (typeof LINUX_DESKTOPS)[number]

/** Who takes a combo first: a named macOS or Windows feature, or the Linux desktops that bind it. */
export type SystemReservation =
  | { os: 'macos' | 'windows'; featureKey: MessageKey }
  | { os: 'linux'; desktops: LinuxDesktop[] }

/** macOS's own shortcuts, by the feature that takes each one. */
const MACOS_FEATURES: Readonly<Partial<Record<string, MessageKey>>> = {
  '⌘Space': 'shortcuts.system.spotlight',
  '⌘⌥Space': 'shortcuts.system.finderSearch',
  '⌘⌃Space': 'shortcuts.system.characterViewer',
  '⌃Space': 'shortcuts.system.inputSourceSwitching',
  '⌘Tab': 'shortcuts.system.appSwitcher',
  '⌃↑': 'shortcuts.system.missionControl',
  '⌃↓': 'shortcuts.system.appWindows',
  '⌃←': 'shortcuts.system.spaces',
  '⌃→': 'shortcuts.system.spaces',
  '⌘⇧3': 'shortcuts.system.screenshots',
  '⌘⇧4': 'shortcuts.system.screenshots',
  '⌘⇧5': 'shortcuts.system.screenRecording',
  '⌘⇧Q': 'shortcuts.system.loggingOut',
  '⌘⌃Q': 'shortcuts.system.lockingScreen',
  '⌘⌥Escape': 'shortcuts.system.forceQuit',
}

/**
 * What Windows itself takes (Microsoft Support, "Keyboard shortcuts in Windows", page dated
 * 2026-09-02). Left out: the bare `Ctrl+Shift` layout switch and `Alt+<underlined letter>`, which
 * aren't combos the recorder captures.
 */
const WINDOWS_FEATURES: Readonly<Partial<Record<string, MessageKey>>> = {
  'Alt+Tab': 'shortcuts.system.switchingApps',
  'Ctrl+Alt+Tab': 'shortcuts.system.switchingApps',
  'Alt+Escape': 'shortcuts.system.cyclingWindows',
  'Alt+Space': 'shortcuts.system.windowMenu',
  'Alt+F4': 'shortcuts.system.closingWindow',
  'Ctrl+Escape': 'shortcuts.system.startMenu',
  'Ctrl+Shift+Escape': 'shortcuts.system.taskManager',
  'Ctrl+Alt+Delete': 'shortcuts.system.securityScreen',
  'Ctrl+Space': 'shortcuts.system.inputMethodToggle',
}

const fKeys = (prefix: string, from: number, to: number): string[] =>
  Array.from({ length: to - from + 1 }, (_, i) => `${prefix}F${String(from + i)}`)
const arrows = (prefix: string, keys = ['↑', '↓', '←', '→']): string[] => keys.map((key) => prefix + key)

/** GNOME's defaults (gsettings-desktop-schemas, gnome-shell, mutter, gnome-settings-daemon). Super combos aside. */
const GNOME_KEYS: readonly string[] = [
  'Alt+F2',
  'Alt+F4',
  'Alt+F5',
  'Alt+F6',
  'Alt+F7',
  'Alt+F8',
  'Alt+F10',
  'Alt+Space',
  'Alt+Tab',
  'Alt+Escape',
  ...arrows('Ctrl+Alt+'),
  ...arrows('Ctrl+Alt+Shift+'),
  ...fKeys('Ctrl+Alt+', 1, 12),
  'Ctrl+Alt+Tab',
  'Ctrl+Alt+Escape',
  'Ctrl+Alt+Delete',
  'Ctrl+Alt+Shift+R',
]

/** Each listed desktop's defaults that an app could otherwise bind. */
const LINUX_DESKTOP_KEYS: Readonly<Record<LinuxDesktop, readonly string[]>> = {
  GNOME: GNOME_KEYS,
  // KDE Plasma 6 (kwin, plasma-workspace, kscreenlocker). Meta combos aside.
  'KDE Plasma': [
    'Alt+F1',
    'Alt+F2',
    'Alt+Shift+F2',
    'Alt+F3',
    'Alt+F4',
    'Alt+Space',
    ...fKeys('Ctrl+', 1, 4),
    'Ctrl+F7',
    'Ctrl+F9',
    'Ctrl+F10',
    'Ctrl+F12',
    'Ctrl+Alt+Delete',
    'Ctrl+Alt+T',
  ],
  // Xfce (libxfce4ui's xfce4-keyboard-shortcuts.xml). The keypad placement keys aside.
  Xfce: [
    'Alt+F1',
    'Alt+F2',
    'Alt+F3',
    'Alt+F4',
    ...fKeys('Alt+', 6, 12),
    ...fKeys('Ctrl+', 1, 12),
    ...arrows('Ctrl+Alt+'),
    ...arrows('Ctrl+Alt+Shift+', ['←', '→', '↑']),
    'Ctrl+Alt+Home',
    'Ctrl+Alt+End',
    'Ctrl+Alt+T',
    'Ctrl+Alt+F',
    'Ctrl+Alt+Delete',
    'Ctrl+Alt+Escape',
    'Ctrl+Escape',
    'Ctrl+Shift+Escape',
    'Alt+Insert',
    'Alt+Delete',
    'Alt+Shift+PageUp',
    'Alt+Shift+PageDown',
    'Alt+Space',
  ],
  // Cinnamon (cinnamon-desktop).
  Cinnamon: [
    'Alt+F1',
    'Alt+F2',
    'Alt+F4',
    'Alt+F5',
    'Alt+F7',
    'Alt+F8',
    'Alt+F10',
    'Alt+Space',
    ...arrows('Ctrl+Alt+'),
    ...arrows('Ctrl+Alt+Shift+'),
    'Ctrl+Alt+L',
    'Ctrl+Alt+T',
    'Ctrl+Alt+End',
    'Ctrl+Alt+Escape',
    'Ctrl+Alt+Delete',
    'Ctrl+Alt+Tab',
    'Ctrl+Alt+Shift+R',
  ],
  // Ubuntu is GNOME plus its own (the ubuntu-settings override, the terminal patch). Keypad keys aside.
  Ubuntu: [...GNOME_KEYS, 'Ctrl+Alt+T', 'Ctrl+Alt+D'],
}

/** Combo → every listed desktop that takes it, in `LINUX_DESKTOPS` order. Built once, on first use. */
let linuxDesktopsByCombo: Map<string, LinuxDesktop[]> | null = null

function linuxDesktopsTaking(combo: string): LinuxDesktop[] {
  linuxDesktopsByCombo ??= LINUX_DESKTOPS.reduce((byCombo, desktop) => {
    for (const key of LINUX_DESKTOP_KEYS[desktop]) byCombo.set(key, [...(byCombo.get(key) ?? []), desktop])
    return byCombo
  }, new Map<string, LinuxDesktop[]>())
  return linuxDesktopsByCombo.get(combo) ?? []
}

/** Who usually takes `combo` on `platform` before Cmdr sees it, or `null` when nothing listed does. */
export function systemReservation(combo: string, platform: Platform = getPlatform()): SystemReservation | null {
  if (platform === 'linux') {
    const desktops = linuxDesktopsTaking(combo)
    return desktops.length > 0 ? { os: 'linux', desktops } : null
  }
  const featureKey = (platform === 'macos' ? MACOS_FEATURES : WINDOWS_FEATURES)[combo]
  return featureKey === undefined ? null : { os: platform, featureKey }
}

/** Every combo the table lists for `platform`, for the checks that walk it. */
export function reservedCombos(platform: Platform): string[] {
  if (platform === 'linux') return [...new Set(LINUX_DESKTOPS.flatMap((desktop) => LINUX_DESKTOP_KEYS[desktop]))]
  return Object.keys(platform === 'macos' ? MACOS_FEATURES : WINDOWS_FEATURES)
}
