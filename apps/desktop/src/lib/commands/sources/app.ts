/**
 * App scope command sources. Pure data (i18n message keys, not English); see
 * `../command-registry.ts` for how the scope arrays are concatenated into the
 * registry and resolved into `Command`s.
 */
import type { CommandSource } from '../types'
import { getBadgeStatus } from '$lib/feature-status'

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
    nativeShortcut: true,
  },
  {
    id: 'app.hide',
    nameKey: 'commands.appHide.label',
    scope: 'App',
    showInPalette: false,
    shortcuts: ['⌘H'],
    nativeShortcut: true,
  },
  {
    id: 'app.hideOthers',
    nameKey: 'commands.appHideOthers.label',
    scope: 'App',
    showInPalette: false,
    shortcuts: ['⌥⌘H'],
    nativeShortcut: true,
  },
  {
    id: 'app.showAll',
    nameKey: 'commands.appShowAll.label',
    scope: 'App',
    showInPalette: false,
    shortcuts: [],
    nativeShortcut: true,
  },
  { id: 'app.about', nameKey: 'commands.appAbout.label', scope: 'App', showInPalette: true, shortcuts: [] },
  // `app.licenseKey` resolves its name from one of two keys via the license-state
  // getter below (see `resolveCommand`), so it carries no `nameKey` here.
  {
    id: 'app.licenseKey',
    nameKey: 'commands.appLicenseKey.seeDetails.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'app.commandPalette',
    nameKey: 'commands.appCommandPalette.label',
    scope: 'App',
    showInPalette: false, // Don't show the palette in itself
    shortcuts: ['⌘⇧P'],
  },
  { id: 'app.settings', nameKey: 'commands.appSettings.label', scope: 'App', showInPalette: true, shortcuts: ['⌘,'] },
  {
    id: 'app.checkForUpdates',
    nameKey: 'commands.appCheckForUpdates.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    descriptionKey: 'commands.appCheckForUpdates.description',
  },
  {
    id: 'cmdr.openOnboarding',
    nameKey: 'commands.cmdrOpenOnboarding.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    descriptionKey: 'commands.cmdrOpenOnboarding.description',
  },
  {
    id: 'help.openShortcuts',
    nameKey: 'commands.helpOpenShortcuts.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    descriptionKey: 'commands.helpOpenShortcuts.description',
  },
  {
    id: 'queue.show',
    nameKey: 'commands.queueShow.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    descriptionKey: 'commands.queueShow.description',
  },
  {
    id: 'help.sendErrorReport',
    nameKey: 'commands.helpSendErrorReport.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    descriptionKey: 'commands.helpSendErrorReport.description',
  },
  {
    id: 'help.whatsNew',
    nameKey: 'commands.helpWhatsNew.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    descriptionKey: 'commands.helpWhatsNew.description',
  },
  {
    id: 'feedback.send',
    nameKey: 'commands.feedbackSend.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    descriptionKey: 'commands.feedbackSend.description',
  },
  {
    // Default ⌘⌥L. The plan's first pick ⌥⌘O is already bound to `file.showInFinder`,
    // so L (for "log") is the mnemonic. Modifier order is Command-then-Option (⌘⌥),
    // matching what `formatKeyCombo` emits, so the JS keydown dispatch fires it on
    // macOS in addition to the native menu accelerator (⌥⌘-order defaults are
    // native-menu-only). See `shortcut-dispatch.ts`.
    id: 'log.operationLog',
    nameKey: 'commands.logOperationLog.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: ['⌘⌥L'],
    descriptionKey: 'commands.logOperationLog.description',
  },
  {
    // Default ⌘⌥A ("A for Ask"). Command-then-Option order (⌘⌥), matching what
    // `formatKeyCombo` emits, so the JS keydown dispatch fires it on macOS too
    // (⌥⌘-order defaults are native-menu-only). macOS still renders it ⌥⌘A in the menu.
    id: 'askCmdr.toggle',
    nameKey: 'commands.askCmdrToggle.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: ['⌘⌥A'],
    descriptionKey: 'commands.askCmdrToggle.description',
    status: getBadgeStatus('ask-cmdr'),
  },
]
