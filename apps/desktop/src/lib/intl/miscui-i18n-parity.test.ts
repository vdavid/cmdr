/**
 * Base-locale (en) parity net for the misc-UI i18n tranche: the `ai`, `ui`,
 * `mtp`, `updates`, and `whatsNew` areas (plus the cloud/local AI settings
 * sections, whose section-specific copy lives in `ai.json`).
 *
 * This is a behavior-preserving MOVE of already-human-authored copy into the
 * catalog: every rendered en string must be byte-identical to the pre-migration
 * literal. The goldens below are those exact literals; if a future copy edit is
 * intended it lands in the catalog AND here together, never silently. The
 * multi-variable (`select`/`plural`/interpolation) and `<tag>` cases get extra
 * coverage because those are where ICU could drift from the old hand-built
 * strings.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { t, tString } from '$lib/intl/messages.svelte'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('ui area parity (en)', () => {
  it('resolves static primitive copy', () => {
    expect(tString('ui.alertDialog.defaultButton')).toBe('OK')
    expect(tString('ui.modalDialog.close')).toBe('Close')
    expect(tString('ui.popover.defaultAriaLabel')).toBe('Options')
    expect(tString('ui.select.placeholder')).toBe('Select...')
    expect(tString('ui.combobox.emptyText')).toBe('No matches. Keep typing to use your own value.')
    expect(tString('ui.toast.sendErrorReport')).toBe('Send error report…')
    expect(tString('ui.toast.dismissAria')).toBe('Dismiss notification')
    expect(tString('ui.commandBox.copy')).toBe('Copy')
    expect(tString('ui.commandBox.copied')).toBe('Copied!')
  })

  it('resolves the interpolated shortcut-chip aria-label', () => {
    expect(t('ui.shortcutChip.customizeAria', { commandName: 'Copy' })).toBe('Customize the Copy shortcut')
  })

  it('resolves the loading-icon plurals (one/other) byte-identically', () => {
    expect(t('ui.loadingIcon.finalizing', { countText: '1', count: 1 })).toBe(
      'All 1 file loaded. Sorting your files, preparing view...',
    )
    expect(t('ui.loadingIcon.finalizing', { countText: '1,234', count: 1234 })).toBe(
      'All 1,234 files loaded. Sorting your files, preparing view...',
    )
    expect(t('ui.loadingIcon.loaded', { countText: '1', count: 1 })).toBe('Loaded 1 file...')
    expect(t('ui.loadingIcon.loaded', { countText: '42', count: 42 })).toBe('Loaded 42 files...')
  })

  it('renders the cancel hint with an empty <key> tag (chip filled by the app)', () => {
    const parts = t('ui.loadingIcon.cancelHint', { key: () => ({ marker: true }) }) as unknown[]
    expect(parts[0]).toBe('Press ')
    expect(parts[parts.length - 1]).toBe(' to cancel and go back')
  })
})

describe('ai area parity (en)', () => {
  it('resolves the toast lifecycle copy', () => {
    expect(tString('ai.toast.downloadingTitle')).toBe('Downloading AI model...')
    expect(tString('ai.toast.startingDownload')).toBe('Starting download...')
    expect(tString('ai.toast.installingTitle')).toBe('Setting up AI...')
    expect(tString('ai.toast.installingDescription')).toBe('Starting server')
    expect(tString('ai.toast.readyTitle')).toBe('AI ready')
    expect(tString('ai.toast.readyDescription')).toBe(
      'Try creating a new folder (F7) to see AI-powered name suggestions.',
    )
    expect(tString('ai.toast.gotIt')).toBe('Got it')
    expect(tString('ai.toast.startingTitle')).toBe('AI starting...')
    expect(tString('ai.toast.startingDescription')).toBe('Loading the model, this takes a few seconds')
    expect(tString('ai.toast.downloadCloseTooltip')).toBe(
      'Close this notification; the download will continue in the background',
    )
  })

  it('resolves the progress line with and without an ETA (the select branch)', () => {
    expect(
      tString('ai.toast.progress', {
        percentText: '12%',
        downloaded: '500.0 KB',
        total: '4.0 MB',
        speed: '100.0 KB',
        eta: '35s',
      }),
    ).toBe('12% · 500.0 KB / 4.0 MB · 100.0 KB/s · 35s remaining')
    expect(
      tString('ai.toast.progress', {
        percentText: '50%',
        downloaded: '2.0 MB',
        total: '4.0 MB',
        speed: '100.0 KB',
        eta: 'none',
      }),
    ).toBe('50% · 2.0 MB / 4.0 MB · 100.0 KB/s')
  })

  it('preserves apostrophes in the translate-error copy', () => {
    expect(tString('ai.translateError.rateLimited.body')).toBe(
      "It's rate-limiting requests or your plan is out of quota. Check your plan and billing, then try again.",
    )
    expect(tString('ai.translateError.unavailable.title')).toBe("Can't reach your AI provider")
    expect(tString('ai.translateError.parseError.title')).toBe("Couldn't read the AI's answer")
    expect(tString('ai.translateError.authFailed.body')).toBe(
      'Check your key in Settings > AI - it might be wrong or revoked.',
    )
  })

  it('resolves the local AI section dynamic copy', () => {
    expect(tString('ai.local.statusRunning')).toBe('Running')
    expect(tString('ai.local.installStepExtracting')).toBe('Step 1 of 4: Extracting runtime...')
    expect(t('ai.local.etaSeconds', { value: '5' })).toBe('~5 sec left')
    expect(t('ai.local.etaMinutes', { value: '3' })).toBe('~3 min left')
    expect(t('ai.local.etaHours', { value: '1' })).toBe('~1 hr left')
    expect(t('ai.local.notInstalled', { modelName: 'Ministral 3B', modelSize: '2.0 GB' })).toBe(
      'Not installed. The local model (Ministral 3B, 2.0 GB) runs entirely on your device for maximum privacy. Requires Apple Silicon.',
    )
    expect(t('ai.local.ramLegendSystem', { size: '4.0 GB' })).toBe('System 4.0 GB')
    expect(t('ai.local.deleteConfirmMessage', { modelSize: '2.0 GB' })).toBe(
      "This frees up 2.0 GB of disk space. You'll need to re-download it to use local AI again.",
    )
  })

  it('resolves the cloud AI section copy', () => {
    expect(tString('ai.cloud.connectedNoModels')).toBe('Connected (model list not available)')
    expect(tString('ai.cloud.connectionError')).toBe("Can't reach server")
    expect(t('ai.cloud.modelPlaceholderExample', { model: 'gpt-4.1-mini' })).toBe('Example: gpt-4.1-mini')
    expect(tString('ai.cloud.apiKeyPlaceholderAnthropic')).toBe('Example: sk-ant-abc123...')
  })
})

describe('mtp area parity (en)', () => {
  it('resolves the connected-toast copy', () => {
    expect(t('mtp.connectedToast.title', { deviceName: 'Pixel 7' })).toBe('Connected to Pixel 7')
    expect(tString('mtp.connectedToast.bodyMac')).toBe(
      'Cmdr paused the macOS camera daemon (ptpcamerad) to access this device. To use it in another app, disable MTP support in settings.',
    )
    expect(tString('mtp.connectedToast.dontShowAgain')).toBe("Don't show again")
    expect(tString('mtp.connectedToast.disableMtp')).toBe('Disable MTP...')
    expect(tString('mtp.deviceFallbackName')).toBe('MTP device')
  })

  it('preserves apostrophes in the permission dialog copy', () => {
    expect(tString('mtp.permissionDialog.title')).toBe("Can't access USB device")
    expect(tString('mtp.permissionDialog.description')).toBe(
      "Cmdr doesn't have permission to access this device. Linux needs udev rules to grant MTP device access.",
    )
  })

  it('renders the ptpcamerad inline-component sentences (the <Trans> cases)', () => {
    // The tag (`<processName>`) and the param (`{process}`) must stay distinctly
    // named: `Trans` merges the tag handlers into the params, so a shared name
    // makes the handler win and the process name never reaches the sentence.
    const inUse = t('mtp.ptpcameradDialog.inUseBy', {
      processName: (chunks: unknown[]) => ({ marker: true, chunks }),
      process: 'pid 45145, ptpcamerad',
    }) as unknown[]
    expect(inUse[0]).toBe('The device is in use by ')
    expect(inUse[1]).toEqual({ marker: true, chunks: ['pid 45145, ptpcamerad'] })
    expect(inUse[inUse.length - 1]).toBe('.')

    const explanation = t('mtp.ptpcameradDialog.explanation', {
      code: () => ({ marker: true }),
    }) as unknown[]
    expect(explanation[0]).toBe('On macOS, the system daemon ')
    expect(explanation[explanation.length - 1]).toBe(
      ' automatically claims Android devices. To work around this, run the following command in Terminal (keep it running while using Cmdr):',
    )

    const help = t('mtp.ptpcameradDialog.helpText', { key: () => ({ marker: true }) }) as unknown[]
    expect(help[0]).toBe('This command continuously stops ptpcamerad while running. Press ')
    expect(help[help.length - 1]).toBe(' in Terminal to stop it when done.')
  })

  it('resolves the device-error strings with and without a blocking process', () => {
    expect(tString('mtp.error.exclusiveAccess', { blocking: 'none' })).toBe('Another process has exclusive access')
    expect(tString('mtp.error.exclusiveAccess', { blocking: 'pid 45145, ptpcamerad' })).toBe(
      'Another process has exclusive access (blocked by pid 45145, ptpcamerad)',
    )
    expect(tString('mtp.error.permissionDenied')).toBe('USB permission denied. Install udev rules and reconnect')
  })
})

describe('updates area parity (en)', () => {
  it('resolves the toast copy', () => {
    expect(tString('updates.toast.available')).toBe('New version available. Restart to update.')
    expect(tString('updates.toast.later')).toBe('Later')
    expect(tString('updates.toast.restart')).toBe('Restart')
    expect(t('updates.checkToast.errorPrefix', { message: 'boom' })).toBe('Error: boom')
  })

  it('resolves the status-line composer byte-identically', () => {
    expect(t('updates.status.noUpdates', { version: '0.23.1' })).toBe('No updates found. Current version: v0.23.1')
    expect(tString('updates.status.checking')).toBe('Checking…')
    expect(t('updates.status.downloading', { next: '0.24.0', prev: '0.23.1' })).toBe(
      'Update found, downloading v0.24.0 (current: v0.23.1)…',
    )
    expect(t('updates.status.installing', { next: '0.24.0', prev: '0.23.1' })).toBe(
      'Installing v0.24.0 (current: v0.23.1)…',
    )
    expect(t('updates.status.ready', { next: '0.24.0' })).toBe('Update v0.24.0 ready. Restart to apply.')
  })
})

describe('whatsNew area parity (en)', () => {
  it('resolves the dialog copy (with the curly apostrophe in the title)', () => {
    expect(tString('whatsNew.dialog.title')).toBe('What’s new in Cmdr')
    expect(tString('whatsNew.dialog.empty')).toBe(
      'Nothing to see here yet. New changes will show up here after an update.',
    )
    expect(tString('whatsNew.dialog.seeFullChangelog')).toBe('See full changelog')
    expect(tString('whatsNew.dialog.optOut')).toBe('Not interested in changelogs')
    expect(tString('whatsNew.optOutToast')).toBe(
      'Got it, no more update notes. Re-enable them anytime in Settings > Updates & privacy.',
    )
  })
})
