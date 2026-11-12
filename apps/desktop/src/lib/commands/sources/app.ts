/**
 * App scope command sources. Pure data (i18n message keys, not English); see
 * `../command-registry.ts` for how the scope arrays are concatenated into the
 * registry and resolved into `Command`s.
 */
import type { CommandSource } from '../types'
import { cmdOrCtrl, macOnly, sameOnAll } from '../platform-shortcuts'
import type { CliCommandState } from '#lib/ipc/bindings.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { getBadgeStatus } from '#lib/feature-status.js'
import { BLOCKED_BY_DIALOGS, runsOverDialogs } from '../while-dialog-open'

// The Hide trio exists only where AppKit runs it from the app menu, so it never reaches the
// dispatch core and is inert to whatever's up in the main window.
const NATIVE_MENU_BAR_ITEM = runsOverDialogs(
  'macOS runs it from the menu bar as a predefined item, so it never reaches the dispatch core.',
)
// Quit is AppKit's item on macOS. Elsewhere it's Cmdr's own command, and it still runs over a
// dialog: the quit gate asks about work in flight, and a stuck dialog must never trap anyone.
const QUIT = runsOverDialogs(
  'The quit gate asks about work in flight, so no dialog blocks quitting. On macOS AppKit runs it from the menu bar.',
)
const OPENS_OWN_WINDOW = runsOverDialogs(
  'Opens its own window, so the dialog in the main window stays up and untouched.',
)
const OPENS_OWN_WINDOW_STEERING = runsOverDialogs(
  'Opens its own window, and steering a running operation is what people reach for while its progress dialog is up.',
)

/**
 * Whether the user already has a license, picking `app.licenseKey`'s label
 * between "See license details" and "Enter license key". It lives here, next to
 * the entry that reads it, because that entry's `nameKey` thunk is the only
 * reader. Kept in sync with the native menu's license item, which resolves its
 * own label through Rust's `Label::License` — a separate mechanism (see
 * `../CLAUDE.md`).
 */
let hasExistingLicense = true

/**
 * Update the license command name based on whether a license exists, keeping the
 * palette in sync with the native menu label. The `nameKey` thunk reads the flag
 * at access time, so flipping it re-resolves the catalog label on the next read
 * with nothing to invalidate. Re-exported from `../command-registry.ts`, which is
 * where callers import it from.
 */
export function updateLicenseCommandName(hasLicense: boolean): void {
  hasExistingLicense = hasLicense
}

/**
 * Where the `cmdr` terminal command stands, picking `app.toggleCliCommand`'s label
 * between Install, Uninstall, and Homebrew's. `#lib/cli-command/` writes it from every
 * status it reads; the native menu item reads the same filesystem fact through Rust
 * (`src-tauri/src/cli/commands.rs`), a separate mechanism like the license's.
 */
let cliCommandState: CliCommandState = 'notInstalled'

export function updateCliCommandName(state: CliCommandState): void {
  cliCommandState = state
}

function cliCommandNameKey(): MessageKey {
  if (cliCommandState === 'installed') return 'commands.appToggleCliCommand.uninstall.label'
  if (cliCommandState === 'installedByHomebrew') return 'commands.appToggleCliCommand.homebrew.label'
  return 'commands.appToggleCliCommand.install.label'
}

export const appCommands: CommandSource[] = [
  // ============================================================================
  // App scope (work everywhere, regardless of window/modal state)
  // ============================================================================
  // The app menu's four (`NATIVE_SHORTCUT_COMMAND_IDS`). Where the OS has an app menu (`appMenu`,
  // macOS), PredefinedMenuItems run them through AppKit selectors (terminate:, hide:,
  // hideOtherApplications:, unhideAllApplications:) with their own keys: `ownedByAppMenu` keeps them
  // out of the palette and the JS dispatch map, and makes the editor read-only and the store refuse
  // to rebind them. Without an app menu the Hide trio doesn't exist, and Quit is Cmdr's own command.
  {
    id: 'app.quit',
    nameKey: 'commands.appQuit.label',
    scope: 'App',
    // Read through `ownedByAppMenu`, so it's in the palette only where no app menu owns it.
    showInPalette: true,
    // Windows and Linux bind nothing: `Alt+F4` closes the window through the quit gate there.
    shortcuts: macOnly('⌘Q'),
    whileDialogOpen: QUIT,
    nativeShortcut: true,
  },
  {
    id: 'app.hide',
    nameKey: 'commands.appHide.label',
    scope: 'App',
    showInPalette: false,
    shortcuts: macOnly('⌘H'),
    whileDialogOpen: NATIVE_MENU_BAR_ITEM,
    nativeShortcut: true,
    needsCapability: 'appMenu',
  },
  {
    id: 'app.hideOthers',
    nameKey: 'commands.appHideOthers.label',
    scope: 'App',
    showInPalette: false,
    shortcuts: macOnly('⌥⌘H'),
    whileDialogOpen: NATIVE_MENU_BAR_ITEM,
    nativeShortcut: true,
    needsCapability: 'appMenu',
  },
  {
    id: 'app.showAll',
    nameKey: 'commands.appShowAll.label',
    scope: 'App',
    showInPalette: false,
    shortcuts: sameOnAll(),
    whileDialogOpen: NATIVE_MENU_BAR_ITEM,
    nativeShortcut: true,
    needsCapability: 'appMenu',
  },
  {
    id: 'app.about',
    nameKey: 'commands.appAbout.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'app.acknowledgements',
    nameKey: 'commands.appAcknowledgements.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'app.licenseKey',
    // Two labels, one command: a thunk, so the `name` getter picks the catalog
    // key from live license state every time it's read.
    nameKey: () =>
      hasExistingLicense ? 'commands.appLicenseKey.seeDetails.label' : 'commands.appLicenseKey.enterKey.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'app.commandPalette',
    nameKey: 'commands.appCommandPalette.label',
    scope: 'App',
    showInPalette: false, // Don't show the palette in itself
    shortcuts: cmdOrCtrl('⌘⇧P'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'app.settings',
    nameKey: 'commands.appSettings.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘,'),
    whileDialogOpen: OPENS_OWN_WINDOW,
  },
  {
    id: 'app.checkForUpdates',
    nameKey: 'commands.appCheckForUpdates.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.appCheckForUpdates.description',
  },
  {
    id: 'cmdr.openOnboarding',
    nameKey: 'commands.cmdrOpenOnboarding.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.cmdrOpenOnboarding.description',
  },
  {
    // Installs or removes the `cmdr` terminal command, whichever the filesystem says is next.
    // Behind `cliCommand` (macOS today), like the link and the script it points at.
    id: 'app.toggleCliCommand',
    nameKey: cliCommandNameKey,
    scope: 'App',
    needsCapability: 'cliCommand',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.appToggleCliCommand.description',
    keywords: ['terminal', 'shell', 'path', 'cli', 'command line'],
  },
  {
    id: 'help.openShortcuts',
    nameKey: 'commands.helpOpenShortcuts.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: OPENS_OWN_WINDOW,
    descriptionKey: 'commands.helpOpenShortcuts.description',
  },
  {
    // Opens getcmdr.com/trust/accessibility-guide in the browser, so it never blocks on a dialog.
    id: 'help.accessibility',
    nameKey: 'commands.helpAccessibility.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: OPENS_OWN_WINDOW,
    descriptionKey: 'commands.helpAccessibility.description',
    keywords: ['accessibility', 'a11y', 'voiceover', 'screen reader', 'help'],
  },
  {
    // Default ⌘⌥Q ("Q for queue"), pairing with `log.operationLog`'s ⌘⌥L next to it in
    // the View menu. Command-then-Option order (⌘⌥) because that's what `formatKeyCombo`
    // emits; Apple's ⌥⌘ display order would be dead on the keyboard. Pinned by
    // `shortcuts/shortcut-vocabulary.test.ts`.
    id: 'queue.show',
    nameKey: 'commands.queueShow.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: { macos: ['⌘⌥Q'], windows: ['Ctrl+Shift+Q'], linux: ['Ctrl+Shift+Q'] },
    whileDialogOpen: OPENS_OWN_WINDOW_STEERING,
    descriptionKey: 'commands.queueShow.description',
  },
  {
    id: 'help.viewDebugLog',
    nameKey: 'commands.helpViewDebugLog.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: OPENS_OWN_WINDOW,
    descriptionKey: 'commands.helpViewDebugLog.description',
  },
  {
    id: 'help.sendErrorReport',
    nameKey: 'commands.helpSendErrorReport.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.helpSendErrorReport.description',
  },
  {
    id: 'help.whatsNew',
    nameKey: 'commands.helpWhatsNew.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.helpWhatsNew.description',
  },
  {
    id: 'feedback.send',
    nameKey: 'commands.feedbackSend.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.feedbackSend.description',
  },
  {
    // Default ⌘⌥L. The first pick ⌘⌥O is already bound to `file.showInFinder`, so L
    // (for "log") is the mnemonic. Modifier order is Command-then-Option (⌘⌥) because
    // that's what `formatKeyCombo` emits; Apple's ⌥⌘ display order would be dead on
    // the keyboard. Pinned by `shortcuts/shortcut-vocabulary.test.ts`.
    id: 'log.operationLog',
    nameKey: 'commands.logOperationLog.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: { macos: ['⌘⌥L'], windows: ['Ctrl+Shift+L'], linux: ['Ctrl+Shift+L'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.logOperationLog.description',
  },
  {
    // No default shortcut. The indicator in the status corner is the everyday way in, and the
    // suggestions wait indefinitely, so this isn't a key anyone reaches for mid-task; a user
    // who wants one can bind it.
    id: 'suggestedOps.show',
    nameKey: 'commands.suggestedOpsShow.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.suggestedOpsShow.description',
  },
  {
    // Default ⌘⌥A ("A for Ask"). Command-then-Option order (⌘⌥), matching what
    // `formatKeyCombo` emits; macOS still renders it ⌥⌘A in the native menu.
    id: 'askCmdr.toggle',
    nameKey: 'commands.askCmdrToggle.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: { macos: ['⌘⌥A'], windows: ['Ctrl+Shift+K'], linux: ['Ctrl+Shift+K'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.askCmdrToggle.description',
    status: getBadgeStatus('ask-cmdr'),
  },
]
