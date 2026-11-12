/**
 * Handlers for the app-level dialog and window openers: the command palette,
 * search, "Go to path", settings, about, license key, error report, feedback,
 * "What's new", updates, onboarding re-entry, the `cmdr` command's install
 * toggle, and the about-window website/upgrade/close actions.
 *
 * The selection-dialog openers (`selection.selectFiles` / `selection.deselectFiles`)
 * live in `selection-handlers`, not here.
 */
import { openDebugLog } from '#lib/logging/open-debug-log.js'
import { openExternalUrl, quitApp } from '#lib/tauri-commands/index.js'
import { openSettingsWindow } from '#lib/settings/settings-window.js'
import { openShortcutsWindow } from '#lib/shortcuts/shortcuts-window.js'
import { openQueueWindow } from '#lib/file-operations/queue/queue-window.js'
import { openErrorReportDialog } from '#lib/error-reporter/error-report-flow.svelte.js'
import { openFeedbackDialog } from '#lib/feedback/feedback-flow.svelte.js'
import { openAcknowledgements } from '#lib/licensing/acknowledgements-trigger.svelte.js'
import { openWhatsNew } from '#lib/whats-new/whats-new-trigger.svelte.js'
import { openOperationLog } from '#lib/operation-log/operation-log-trigger.svelte.js'
import { openSuggestedOps } from '#lib/suggested-ops/suggested-ops-trigger.svelte.js'
import { openNewChat, toggleRail } from '#lib/ask-cmdr/ask-cmdr-trigger.svelte.js'
import { runMenuTriggeredCheck } from '#lib/updates/updater.svelte.js'
import { toggleCliCommand } from '#lib/cli-command/toggle-cli-command.js'
import { detached } from './detached'
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
    // `openSettingsWindow` reports `settings_opened` itself, for every surface at once.
    detached(openSettingsWindow('command'))
  },

  'app.about': ({ ctx }) => {
    ctx.dialogs.showAboutWindow(true)
  },

  'app.licenseKey': ({ ctx }) => {
    ctx.dialogs.showLicenseKeyDialog(true)
  },

  'help.openShortcuts': () => {
    detached(openShortcutsWindow())
  },

  'help.accessibility': async () => {
    await openExternalUrl('https://getcmdr.com/trust/accessibility-guide')
  },

  'queue.show': () => {
    detached(openQueueWindow())
  },

  'help.viewDebugLog': () => {
    detached(openDebugLog())
  },

  'help.sendErrorReport': () => {
    openErrorReportDialog({ from: 'helpMenu' })
  },

  'feedback.send': () => {
    openFeedbackDialog()
  },

  'app.acknowledgements': () => {
    openAcknowledgements()
  },

  'help.whatsNew': () => {
    detached(openWhatsNew())
  },

  'log.operationLog': () => {
    detached(openOperationLog())
  },

  'suggestedOps.show': () => {
    detached(openSuggestedOps())
  },

  'askCmdr.toggle': () => {
    toggleRail()
  },

  'askCmdr.newChat': () => {
    openNewChat()
  },

  // Reached only where no app menu owns Quit (Windows and Linux); on macOS AppKit's item runs it.
  // Detached: the backend may hold the quit and ask, and on the way out the call never settles.
  'app.quit': () => {
    detached(quitApp())
  },

  'app.checkForUpdates': () => {
    detached(runMenuTriggeredCheck())
  },

  'cmdr.openOnboarding': async ({ ctx }) => {
    await ctx.dialogs.openOnboarding()
  },

  'app.toggleCliCommand': () => {
    // Detached: an install can sit on macOS's password dialog for as long as the user takes.
    detached(toggleCliCommand())
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
