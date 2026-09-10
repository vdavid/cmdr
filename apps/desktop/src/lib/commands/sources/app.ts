/**
 * App scope command sources. Pure data (i18n message keys, not English); see
 * `../command-registry.ts` for how the scope arrays are concatenated into the
 * registry and resolved into `Command`s.
 */
import type { CommandSource } from '../types'
import { getBadgeStatus } from '$lib/feature-status'
import { BLOCKED_BY_DIALOGS, runsOverDialogs } from '../while-dialog-open'

// macOS owns these four outright via PredefinedMenuItems; they never reach the dispatch core, so
// they're inert to whatever's up in the main window.
const NATIVE_MENU_BAR_ITEM = runsOverDialogs(
  'macOS runs it from the menu bar as a predefined item, so it never reaches the dispatch core.',
)
const OPENS_OWN_WINDOW = runsOverDialogs(
  'Opens its own window, so the dialog in the main window stays up and untouched.',
)
const OPENS_OWN_WINDOW_STEERING = runsOverDialogs(
  'Opens its own window, and steering a running operation is what people reach for while its progress dialog is up.',
)

export const appCommands: CommandSource[] = [
  // ============================================================================
  // App scope (work everywhere, regardless of window/modal state)
  // ============================================================================
  // Native-only: handled by PredefinedMenuItems via macOS selectors (hide:, hideOtherApplications:,
  // unhideAllApplications:, terminate:). showInPalette: false keeps them out of the JS shortcut
  // dispatch map; the native menu accelerators handle the keyboard shortcuts directly. `nativeShortcut`
  // makes the editor read-only and the store refuse to rebind them (NATIVE_SHORTCUT_COMMAND_IDS above).
  {
    id: 'app.quit',
    nameKey: 'commands.appQuit.label',
    scope: 'App',
    showInPalette: false,
    shortcuts: ['⌘Q'],
    whileDialogOpen: NATIVE_MENU_BAR_ITEM,
    nativeShortcut: true,
  },
  {
    id: 'app.hide',
    nameKey: 'commands.appHide.label',
    scope: 'App',
    showInPalette: false,
    shortcuts: ['⌘H'],
    whileDialogOpen: NATIVE_MENU_BAR_ITEM,
    nativeShortcut: true,
  },
  {
    id: 'app.hideOthers',
    nameKey: 'commands.appHideOthers.label',
    scope: 'App',
    showInPalette: false,
    shortcuts: ['⌥⌘H'],
    whileDialogOpen: NATIVE_MENU_BAR_ITEM,
    nativeShortcut: true,
  },
  {
    id: 'app.showAll',
    nameKey: 'commands.appShowAll.label',
    scope: 'App',
    showInPalette: false,
    shortcuts: [],
    whileDialogOpen: NATIVE_MENU_BAR_ITEM,
    nativeShortcut: true,
  },
  {
    id: 'app.about',
    nameKey: 'commands.appAbout.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'app.acknowledgements',
    nameKey: 'commands.appAcknowledgements.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  // `app.licenseKey` resolves its name from one of two keys via the license-state
  // getter below (see `resolveCommand`), so it carries no `nameKey` here.
  {
    id: 'app.licenseKey',
    nameKey: 'commands.appLicenseKey.seeDetails.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'app.commandPalette',
    nameKey: 'commands.appCommandPalette.label',
    scope: 'App',
    showInPalette: false, // Don't show the palette in itself
    shortcuts: ['⌘⇧P'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'app.settings',
    nameKey: 'commands.appSettings.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: ['⌘,'],
    whileDialogOpen: OPENS_OWN_WINDOW,
  },
  {
    id: 'app.checkForUpdates',
    nameKey: 'commands.appCheckForUpdates.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.appCheckForUpdates.description',
  },
  {
    id: 'cmdr.openOnboarding',
    nameKey: 'commands.cmdrOpenOnboarding.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.cmdrOpenOnboarding.description',
  },
  {
    id: 'help.openShortcuts',
    nameKey: 'commands.helpOpenShortcuts.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: OPENS_OWN_WINDOW,
    descriptionKey: 'commands.helpOpenShortcuts.description',
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
    shortcuts: ['⌘⌥Q'],
    whileDialogOpen: OPENS_OWN_WINDOW_STEERING,
    descriptionKey: 'commands.queueShow.description',
  },
  {
    id: 'help.sendErrorReport',
    nameKey: 'commands.helpSendErrorReport.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.helpSendErrorReport.description',
  },
  {
    id: 'help.whatsNew',
    nameKey: 'commands.helpWhatsNew.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.helpWhatsNew.description',
  },
  {
    id: 'feedback.send',
    nameKey: 'commands.feedbackSend.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
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
    shortcuts: ['⌘⌥L'],
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
    shortcuts: [],
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
    shortcuts: ['⌘⌥A'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.askCmdrToggle.description',
    status: getBadgeStatus('ask-cmdr'),
  },
]
