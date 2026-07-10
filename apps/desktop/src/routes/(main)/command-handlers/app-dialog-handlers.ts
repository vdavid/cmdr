/**
 * Handlers for the app-level dialog and window openers: the command palette,
 * search, "Go to path", settings, about, license key, error report, feedback,
 * "What's new", updates, onboarding re-entry, and the about-window
 * website/upgrade/close actions.
 *
 * The selection-dialog openers (`selection.selectFiles` / `selection.deselectFiles`)
 * live in `selection-handlers`, not here.
 */
import { openExternalUrl, trackEvent } from '$lib/tauri-commands'
import { openSettingsWindow } from '$lib/settings/settings-window'
import { openShortcutsWindow } from '$lib/shortcuts/shortcuts-window'
import { openQueueWindow } from '$lib/file-operations/queue/queue-window'
import { openErrorReportDialog } from '$lib/error-reporter/error-report-flow.svelte'
import { openFeedbackDialog } from '$lib/feedback/feedback-flow.svelte'
import { openWhatsNew } from '$lib/whats-new/whats-new-trigger.svelte'
import { openOperationLog } from '$lib/operation-log/operation-log-trigger.svelte'
import { runMenuTriggeredCheck } from '$lib/updates/updater.svelte'
import type { CommandHandlerRecord } from './types'

export const appDialogHandlers = {
  'app.commandPalette': ({ ctx }) => {
    ctx.dialogs.showCommandPalette(true)
  },

  'search.open': ({ ctx }) => {
    ctx.dialogs.showSearchDialog(true)
  },

  'nav.goToPath': ({ ctx }) => {
    ctx.dialogs.showGoToPathDialog(true)
  },

  'app.settings': () => {
    void openSettingsWindow()
    // PII-free analytics: the settings window opened. No props.
    void trackEvent('settings_opened')
  },

  'app.about': ({ ctx }) => {
    ctx.dialogs.showAboutWindow(true)
  },

  'app.licenseKey': ({ ctx }) => {
    ctx.dialogs.showLicenseKeyDialog(true)
  },

  'help.openShortcuts': () => {
    void openShortcutsWindow()
  },

  'queue.show': () => {
    void openQueueWindow()
  },

  'help.sendErrorReport': () => {
    openErrorReportDialog()
  },

  'feedback.send': () => {
    openFeedbackDialog()
  },

  'help.whatsNew': () => {
    void openWhatsNew()
  },

  'log.operationLog': () => {
    void openOperationLog()
  },

  'app.checkForUpdates': () => {
    void runMenuTriggeredCheck()
  },

  'cmdr.openOnboarding': ({ ctx }) => {
    ctx.dialogs.openOnboarding()
  },

  'about.openWebsite': async () => {
    await openExternalUrl('https://getcmdr.com')
  },

  'about.openUpgrade': async () => {
    await openExternalUrl('https://getcmdr.com/upgrade')
  },

  'about.close': ({ ctx }) => {
    ctx.dialogs.showAboutWindow(false)
  },
} satisfies Partial<CommandHandlerRecord>
