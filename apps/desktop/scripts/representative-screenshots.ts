/**
 * Curated REPRESENTATIVE screenshot mappings for `couple-screenshots.ts`,
 * applied AFTER the precise capture-based coupling.
 *
 * A representative coupling is honest-by-design: it says "we have no exact
 * screenshot of YOUR string, but here's a real screenshot of the same
 * panel/toast/dialog where it appears, in the same position", so a translator
 * loads ONE image for a whole family of strings instead of none.
 *
 * They live in their own module because this is a curated data table that grows
 * with the UI, while the coupler around it is machinery that doesn't.
 */

export interface RepresentativeMapping {
  /**
   * Matched with `startsWith`, so a whole KEY works as well as a family prefix
   * when one string needs a note of its own. `representativeFor` takes the first
   * match, so a specific entry has to precede the broader one it lives under.
   */
  prefix: string
  screenshot: string
  note: string
}

export const REPRESENTATIVE_SCREENSHOTS: RepresentativeMapping[] = [
  {
    // The rename / New Folder / New File refusal family, which does NOT use the
    // error pane below: it renders as one plain-text line under the name field
    // (or in a toast), exactly where the live validation messages appear. Listed
    // BEFORE the blanket `errors.` mapping, which would otherwise hand these
    // keys the error-pane note and mislead a translator about their surface.
    prefix: 'errors.mutation.',
    screenshot: 'mkdir-confirmation-too-long.png',
    note:
      'This one is NOT the error panel. Your string is the single red line under the name field in the Rename / New folder / ' +
      'New file box, exactly where the validation message sits in this screenshot (which shows a DIFFERENT message). One ' +
      'sentence, plain text, no markdown, and it has to fit a narrow dialog.',
  },
  {
    // Same surface as `errors.mutation.*`: a volume's refusal is rendered by the
    // same one-line factory when a mutation carries it.
    prefix: 'errors.volume.',
    screenshot: 'mkdir-confirmation-too-long.png',
    note:
      'This one is NOT the error panel. Your string is the single red line under the name field in the Rename / New folder / ' +
      'New file box, exactly where the validation message sits in this screenshot (which shows a DIFFERENT message). One ' +
      'sentence, plain text, no markdown, and it has to fit a narrow dialog.',
  },
  {
    // The whole friendly-error family (listing / write / provider / git) shares
    // one presentation: an error pane (or, for write ops, the same title +
    // explanation + suggestion layout in a dialog). The example shows a DIFFERENT
    // error than yours, but your title/message/suggestion text appears in this
    // same panel, in the same three stacked positions.
    prefix: 'errors.',
    screenshot: 'error-message-example.png',
    note:
      'Cmdr renders every friendly error with one shared layout: a bold title, an explanation paragraph, and a suggestion ' +
      'below it (plus an optional action button and a collapsed "Technical details"). This screenshot shows a DIFFERENT error, ' +
      'but your string appears as the title, explanation, or suggestion text in this same panel, in the same position. ' +
      'errors.provider.* names (Dropbox, Google Drive, OneDrive, and so on) are brand names, so keep them as-is.',
  },
  {
    // SMB / network connect + reconnect + the MTP connection states all live on
    // the network/device browsing surface reached via "Connect to server".
    prefix: 'fileExplorer.network.',
    screenshot: 'connect-to-server.png',
    note:
      'Network (SMB) connection flow. This shows the "Connect to server" surface; your string appears here or on the ' +
      'closely-related browsing/sign-in/reconnect states reached from it.',
  },
  {
    prefix: 'fileExplorer.smbReconnect.',
    screenshot: 'connect-to-server.png',
    note:
      'The SMB reconnect banner shown when a mounted server drops: a "Reconnecting…" title, a countdown, and Retry/Cancel ' +
      'controls. This shows the related "Connect to server" surface; your string appears in the same network-connection context.',
  },
  {
    prefix: 'fileExplorer.networkMount.',
    screenshot: 'connect-to-server.png',
    note: 'Shown while mounting a network share, in the same network-connection flow as the "Connect to server" surface pictured here.',
  },
  {
    // MTP device connection states + dialogs share the MTP browsing context.
    prefix: 'fileExplorer.mtp.',
    screenshot: 'mtp-browse.png',
    note:
      'MTP (phone/camera) connection status shown in the device pane. This shows the MTP browse surface; your string appears ' +
      'as a status message in this same device context (connecting, busy, disconnected, etc.).',
  },
  {
    prefix: 'mtp.',
    screenshot: 'mtp-browse.png',
    note:
      'MTP (phone/camera) device messaging: a connect/permission dialog or toast tied to an MTP device. This shows the MTP ' +
      'browse surface for context. Keep device/protocol names (MTP, PTP) as-is.',
  },
  {
    // The Ask Cmdr model-override hint renders conditionally, so it keeps a precise
    // note explaining the screenshot may not show it (more specific than the `ai.` rule below).
    prefix: 'ai.cloud.askCmdrOverrideHint',
    screenshot: 'settings-ai-ask-cmdr.png',
    note: 'The hint renders under the model picker in the Settings > AI > Ask Cmdr subsection pictured here (only while the Ask Cmdr override is set, so the screenshot may not show it).',
  },
  {
    // AI provider/cloud connection states render in the Settings > AI > Provider subsection.
    // Settings > AI is captured as three separate surfaces (provider, Ask Cmdr, MCP server);
    // the provider one is where connection state lives, so it's the honest stand-in.
    prefix: 'ai.',
    screenshot: 'settings-ai-provider.png',
    note:
      'AI feature copy. Cloud-connection states, suggestions, and translate-errors surface around the Settings > AI > Provider ' +
      'subsection pictured here (and inline near AI actions). This shows the AI provider settings for context.',
  },
  {
    prefix: 'onboarding.cloudSetup.',
    screenshot: 'onboarding-ai.png',
    note: 'Cloud-AI setup copy in the onboarding wizard. This shows the onboarding AI step where these strings render.',
  },
  {
    prefix: 'onboarding.stepAi.',
    screenshot: 'onboarding-ai.png',
    note: 'The AI step of the onboarding wizard, pictured here.',
  },
  {
    // The crash-report dialog reuses the error-report dialog's form.
    prefix: 'crashReporter.',
    screenshot: 'error-report.png',
    note:
      'The crash-report dialog (shown on the next launch after Cmdr hit a problem) uses the same report-form layout as the ' +
      'error-report dialog pictured here: an intro, a privacy note, a copyable report ID, and Send/Cancel buttons.',
  },
  {
    // Every query surface (search, select, filter) is the same `QueryDialog`: a
    // mode row, the query bar, scope + filter controls, and a results list.
    prefix: 'queryUi.',
    screenshot: 'search-dialog.png',
    note:
      'Cmdr’s search, select, and filter surfaces are one shared dialog: a mode row at the top, the query bar, the scope ' +
      'and filter controls under it, and results below. This shows that dialog; your string is one of those controls, one of ' +
      'the filter or date options, or a line in the results area.',
  },
  {
    // The image-text results panel and the search toasts belong to the search flow.
    prefix: 'search.',
    screenshot: 'search-dialog.png',
    note:
      'Search results and the messages around them (image-text matches, index coverage, and the toasts search raises). This ' +
      'shows the search dialog where the query is typed and the results appear.',
  },
  {
    // The details view, the server-invalid banner, and the error codes are all
    // states of the same license dialog.
    prefix: 'licensing.dialog.',
    screenshot: 'license-key-dialog.png',
    note:
      'The license dialog. This shows its key-entry state; your string belongs to one of its other states (the details view ' +
      'for a committed license, the verification banners, or an error message), which render in this same dialog.',
  },
  {
    // The delete confirmation's other shapes (archives, symlinks, overflow lines,
    // live throughput) are the same dialog.
    prefix: 'fileOperations.delete.',
    screenshot: 'delete-confirm.png',
    note:
      'The delete confirmation dialog, pictured here. Your string is one of its variants: a different item count, an archive ' +
      'or symlink warning, or the progress line it shows while scanning.',
  },
  {
    // A single KEY, not a family. Both rollback tooltips sit on the dialog's
    // Rollback button, so the phase-and-progress-bar note below misdescribes
    // them; they're listed BEFORE it because `representativeFor` takes the first
    // matching prefix. Any future full-key entry under this family goes here too,
    // longest key first, since a shorter key is a prefix of a longer sibling.
    prefix: 'fileOperations.transferProgress.rollbackTooltipStopAndMoveBack',
    screenshot: 'transfer-dialog.png',
    note: 'The copy/move progress dialog, pictured here. Your string is the tooltip on its Rollback button.',
  },
  {
    prefix: 'fileOperations.transferProgress.rollbackAlreadyLandedTooltip',
    screenshot: 'transfer-dialog.png',
    note:
      'The copy/move progress dialog, pictured here. Your string is the tooltip on its Rollback button while the ' +
      'dialog\'s title reads "Removing the originals…".',
  },
  {
    // The transfer progress dialog's other phases (scan, pause, queue, flush).
    prefix: 'fileOperations.transferProgress.',
    screenshot: 'transfer-dialog.png',
    note:
      'The copy/move progress dialog, pictured here. Your string belongs to one of its phases (scanning, paused, queued, or ' +
      'finishing up) or to one of its two progress bars.',
  },
  {
    // The failure notice's SUMMARY form only appears past three failures at once,
    // which no surface stages: `operation-failure` shows the single-failure toast,
    // and both are the same pane in the same corner. `title` and `action` are
    // captured directly there, so the direct pass keeps them and only the summary
    // falls through to this stand-in.
    prefix: 'queue.failureToast.',
    screenshot: 'operation-failure.png',
    note:
      'The notice the main window raises when a backgrounded operation stops early, pictured here with one failure. Your ' +
      'string is the summary that replaces these when several stop at once, in the same place, with a count instead of a name.',
  },
  {
    // The update status line lives in Settings > Updates & privacy; the toasts it
    // raises appear over the same panel.
    prefix: 'updates.',
    screenshot: 'settings-updates.png',
    note:
      'App-update messaging. The status line renders in the Settings > Updates & privacy panel pictured here, and the ' +
      'update toasts appear while that check runs. Version numbers are substituted, not translated.',
  },
  {
    // Viewer chrome, load errors, media labels, and status-bar copy all belong to
    // this one window, so a single shot of it stands in for all of them. It points
    // at the find-bar state because that's the viewer surface the capture run keeps
    // (the plain-chrome, image, and PDF states resolved nothing the others didn't).
    prefix: 'viewer.',
    screenshot: 'viewer-search.png',
    note:
      'The file viewer window, pictured here with its find bar open. Your string is part of its chrome (title, toolbar, ' +
      'status bar), one of the file-kind or media labels it shows there, or one of the messages it shows in place of ' +
      'content when a file can’t be loaded.',
  },
  {
    // The operation log's other states (loading, empty, load error) are the same
    // dialog with a different body.
    prefix: 'operationLog.',
    screenshot: 'operation-log-more-pages.png',
    note:
      'The operation log dialog, pictured here with entries in it. Your string belongs to one of its other states (loading, ' +
      'empty, or a load error), which render in this same dialog in place of the list.',
  },
  {
    // The shortcuts window reuses the Settings keyboard-shortcuts list layout.
    prefix: 'shortcuts.',
    screenshot: 'settings-keyboard-shortcuts.png',
    note:
      'Keyboard-shortcut UI. This shows the Settings > Keyboard shortcuts list, which uses the same row/scope/conflict layout ' +
      'as the standalone Shortcuts window. macOS modifier glyphs (⌘ ⌥ ⌃ ⇧) and key names are not translated.',
  },
]
