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
    // The share-mount refusal family doesn't use the error pane below either: it's
    // one plain sentence under the network pane's "Couldn't mount share" title.
    // Listed BEFORE the blanket `errors.` mapping for the same reason as
    // `errors.mutation.`.
    prefix: 'errors.mount.',
    screenshot: 'servers-hub.png',
    note:
      'This one is NOT the error panel. Your string is the one sentence under the "Couldn\'t mount share" title that replaces a ' +
      "server's list of shares when a share doesn't open. That list opens from the servers list pictured here.",
  },
  {
    // The share-listing family: one sentence under the network pane's "Couldn't
    // connect to …" title, and the tooltip on that server's row. Same reason to
    // precede `errors.`.
    prefix: 'errors.shareList.',
    screenshot: 'servers-hub.png',
    note:
      'This one is NOT the error panel. Your string is the one sentence under the "Couldn\'t connect to …" title that ' +
      "replaces a server's list of shares when that list doesn't load. That list opens from the servers list pictured " +
      "here, and the same sentence is the tooltip on that server's row in it.",
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
    // SMB browsing, connect, and reconnect states all live in the network flow
    // that starts at the servers hub.
    prefix: 'fileExplorer.network.',
    screenshot: 'servers-hub.png',
    note:
      'Network (SMB) browsing and connection flow. This shows the servers list it starts from; your string appears here or ' +
      'on the closely related browsing, sign-in, or reconnect states reached from it.',
  },
  {
    // The pane's own connection states (`RemoteConnectView`): connecting, signed
    // out, a changed host key, and the reconnect cycle's countdown.
    prefix: 'servers.paneState.',
    screenshot: 'servers-hub.png',
    note:
      'Shown inside a file pane while it connects to a server: "Connecting…", "Signed out", a changed host key, or a ' +
      '"Reconnecting…" state with a countdown and Retry/Cancel controls. This shows the servers list those connections ' +
      'start from; your string appears in the pane, in the same network context.',
  },
  {
    prefix: 'fileExplorer.networkMount.',
    screenshot: 'servers-hub.png',
    note: 'Shown while mounting a network share, in the network flow that starts at the servers list pictured here.',
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
    prefix: 'fileOperations.transferProgress.titleRemovingOriginals',
    screenshot: 'transfer-dialog.png',
    note:
      'The copy/move progress dialog, pictured here. Your string is the title it shows during the final stage of a move ' +
      'between disks.',
  },
  {
    // All three reversal titles, which the dialog shows only for a rollback
    // started from the operation history.
    prefix: 'fileOperations.transferProgress.titleReversal',
    screenshot: 'transfer-dialog.png',
    note:
      'The copy/move progress dialog, pictured here. Your string is its title while the dialog is showing a rollback ' +
      'launched from the operation history, rather than a copy or a move.',
  },
  {
    // Compressing to a remote place needs a server, so the phases read off the same dialog. The note names them, since no shot shows them.
    prefix: 'fileOperations.transferProgress.stage',
    screenshot: 'transfer-dialog.png',
    note: 'The copy/move progress dialog, pictured here on a plain copy. Your string is the line above its progress bar during a compress: Compressing, then Finishing compression, then Transferring the archive, then Finishing the transfer, each with "Step 1 of 2" or "Step 2 of 2" in front when the archive goes to a remote place.',
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
    // The toast that reports what a cancelled rollback could and couldn't put
    // back. Only this one line has a stand-in; the rest of `cancelRollback.*`
    // stays uncoupled rather than claiming a picture it doesn't have.
    prefix: 'fileOperations.cancelRollback.leftBehind',
    screenshot: 'toast-transfer-complete.png',
    note:
      'This shows a different toast in the same corner of the window; your string is one of the lines inside a toast ' +
      'like it.',
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
    // The rollback cross-reference lines sit under a list row, not in another
    // dialog state. Listed BEFORE the blanket `operationLog.` mapping, whose
    // note would otherwise misplace them. `rollbackOf` also covers `rollbackOfUnlisted`.
    prefix: 'operationLog.dialog.rollbackOf',
    screenshot: 'operation-log-more-pages.png',
    note:
      'The operation log dialog, pictured here with entries in it. Your string is the quiet second line under one of those ' +
      'rows, shown only when that operation is a rollback or has been rolled back, so this screenshot may not show it.',
  },
  {
    prefix: 'operationLog.dialog.latestRollback',
    screenshot: 'operation-log-more-pages.png',
    note:
      'The operation log dialog, pictured here with entries in it. Your string is the quiet second line under one of those ' +
      'rows, shown only when that operation is a rollback or has been rolled back, so this screenshot may not show it.',
  },
  {
    // The recording-off notice sits above the list in every state, not in place of it.
    prefix: 'operationLog.dialog.recordingOff',
    screenshot: 'operation-log-more-pages.png',
    note:
      'The operation log dialog, pictured here with recording on. Your string is a notice at the top of this dialog, ' +
      'above the list, shown in every state while recording is off.',
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
  {
    // The two headings render only while the search box is empty.
    prefix: 'commandPalette.group',
    screenshot: 'command-palette.png',
    note: 'The command palette, pictured here. Your string is one of the two section headings it shows above its list while the search box is empty.',
  },
  {
    // Every other command description is captured on this list; this one's row doesn't render in the shot.
    prefix: 'commands.fileOpenTerminalHere.description',
    screenshot: 'settings-keyboard-shortcuts.png',
    note:
      'The Settings > Keyboard shortcuts list, pictured here. Your string is the one-line description of the "Open terminal ' +
      'here" command, which appears with the other command descriptions in this list and in the command palette.',
  },
  {
    // Tab states (pinned, unreachable) the main-window shot doesn't stage.
    prefix: 'fileExplorer.tabBar.',
    screenshot: 'main-window.png',
    note:
      'The main window, pictured here. Your string belongs to the tab bar above a file pane: the tooltip or spoken name of a ' +
      'tab in a state this screenshot doesn’t show, such as a pinned tab or one pointing at a place that can’t be reached.',
  },
  {
    prefix: 'fileExplorer.columns.created',
    screenshot: 'main-window.png',
    note: 'The main window, pictured here. Your string is the header of the creation date column in the file list, which this screenshot doesn’t show.',
  },
  {
    prefix: 'fileExplorer.diskSpace.used',
    screenshot: 'main-window.png',
    note: 'The main window, pictured here. Your string is one of the disk space figures Cmdr shows for the drive a file pane is on.',
  },
  {
    // The unbounded variant of the free-space line captured on this surface.
    prefix: 'fileOperations.transferDialog.spaceInfoUnbounded',
    screenshot: 'conflict-dialog.png',
    note:
      'The copy/move dialog, pictured here. Your string replaces the free-space line next to the destination picker when the ' +
      'destination has no size limit, such as a cloud account with no quota.',
  },
  {
    // The recent-paths list and the typed-path hints; the capture stages an empty dialog.
    prefix: 'goToPath.dialog.',
    screenshot: 'go-to-path.png',
    note:
      'The Go to folder dialog, pictured here. Your string belongs to a state this screenshot doesn’t show: its list of recent ' +
      'paths, or a hint under the field about the path you typed.',
  },
  {
    // The checklist's phase headers, run-kind headers, long ETAs, and image-indexing row are
    // all states of the same tiles this surface photographs.
    prefix: 'indexing.phase.',
    screenshot: 'indexing-checklist.png',
    note: 'The drive-indexing checklist, pictured here. Your string is the header it shows during one phase of a drive’s first index.',
  },
  {
    prefix: 'indexing.run.',
    screenshot: 'indexing-checklist.png',
    note: 'The drive-indexing checklist, pictured here. Your string is the header naming the kind of run in progress, for a run this screenshot doesn’t show.',
  },
  {
    prefix: 'indexing.eta.',
    screenshot: 'indexing-checklist.png',
    note: 'The drive-indexing checklist, pictured here. Your string is how much time it says is left, in the form it uses for longer waits.',
  },
  {
    prefix: 'indexing.enrich.',
    screenshot: 'indexing-checklist.png',
    note:
      'The drive-indexing checklist, pictured here. Your string belongs to the image-indexing row that appears beside these ' +
      'drive rows while Cmdr reads image contents: its title, progress, speed, or a paused or waiting state.',
  },
  {
    prefix: 'queue.chip.',
    screenshot: 'operation-chip.png',
    note: 'The operation chip in the corner of the main window, pictured here. Your string is never shown: a screen reader speaks it for the chip while an operation is still scanning.',
  },
  {
    // Row states the queue shot doesn't stage: a reversal, a clash waiting for an answer, a paused operation.
    prefix: 'queue.row.',
    screenshot: 'queue.png',
    note:
      'The operations queue window, pictured here. Your string belongs to a row state this screenshot doesn’t show: an undo ' +
      'in progress, an operation waiting for your answer, or a paused one with its Resume button.',
  },
  {
    // Dropdown and swatch options of controls this section shows closed.
    prefix: 'settings.appearance.dateTimeFormat.',
    screenshot: 'settings-appearance.png',
    note: 'Settings > Appearance, pictured here. Your string is one of the date and time format choices, or the example shown under one of them.',
  },
  {
    prefix: 'settings.appearance.language.opt.',
    screenshot: 'settings-appearance.png',
    note: 'Settings > Appearance, pictured here. Your string is a choice in its language picker.',
  },
  {
    prefix: 'settings.listing.briefColumnWidthMode.',
    screenshot: 'settings-appearance-listing.png',
    note: 'Settings > Appearance > Listing, pictured here. Your string is one of the choices for how wide columns in the Brief view may grow.',
  },
  {
    prefix: 'settings.network.timeoutMode.',
    screenshot: 'settings-file-systems-smb.png',
    note: 'Settings > File systems > SMB, pictured here. Your string is one of the network timeout choices, or the line under one of them.',
  },
  {
    prefix: 'settings.askCmdr.',
    screenshot: 'settings-ai-ask-cmdr.png',
    note:
      'Settings > AI > Ask Cmdr, pictured here. Your string belongs to a state this screenshot doesn’t show (off, or asking ' +
      'you to review changed wording), a confirmation or empty state in one of its panels, or a note under one of its controls.',
  },
  {
    prefix: 'whatsNew.dialog.showLess',
    screenshot: 'whats-new.png',
    note: 'The What’s new dialog, pictured here. Your string is the link that folds a release’s detailed changes away again once someone has expanded them.',
  },
  {
    // `download-toast.png` renamed to `toast-download.png`; these are the toast's other forms.
    prefix: 'downloads.toast.',
    screenshot: 'toast-download.png',
    note:
      'The toast Cmdr shows when a download lands, pictured here. Your string is another form of it: the summary when several ' +
      'downloads land at once, the note that one landed in a subfolder, or the tooltip on its expand button.',
  },
  {
    // `feedback-dialog.png` renamed to `feedback.png`; these are the dialog's other states.
    prefix: 'feedback.dialog.',
    screenshot: 'feedback.png',
    note:
      'The feedback dialog, pictured here. Your string belongs to a state this screenshot doesn’t show: its character ' +
      'counter, the note that a message is too long, the Send button while sending, or the line shown when a message ' +
      'couldn’t be sent.',
  },
  {
    // `transfer-complete-toast.png` renamed to `toast-transfer-complete.png`; every `transfer.*` key is an outcome of that toast.
    prefix: 'transfer.',
    screenshot: 'toast-transfer-complete.png',
    note:
      'The toast Cmdr shows when an operation finishes, pictured here. Your string is another outcome of the same toast: a ' +
      'move to trash or a delete, a compress, some files skipped, or files that appeared during a move.',
  },
  {
    // `settings-behavior-file-system-watching.png` went away when drive indexing got its own section.
    prefix: 'settings.indexing.masterOffNote',
    screenshot: 'settings-indexing-drive-indexing.png',
    note: 'Settings > Indexing > Drive indexing, pictured here. Your string is the note under the main switch that appears only while that switch is off.',
  },
  {
    prefix: 'settings.indexing.overriddenBadge',
    screenshot: 'settings-indexing-drive-indexing.png',
    note: 'Settings > Indexing > Drive indexing, pictured here. Your string is the small badge on a row the main switch overrides, shown only while that switch is off.',
  },
  {
    // Only rendered while the downloads watcher lacks Full Disk Access, which the capture machine has.
    prefix: 'common.downloadsFdaHint',
    screenshot: 'settings-behavior-notifications.png',
    note:
      'Settings > Behavior > Notifications, pictured here. Your string is the hint at the top of this section that appears ' +
      'only while Cmdr can’t watch the Downloads folder for lack of Full Disk Access, with a link that opens System Settings.',
  },
  {
    // The tool lines, turn stats, and thread rows no surface reaches (a phone, a server, a state only a real provider gets to) read off the shots of their siblings.
    prefix: 'askCmdr.tool.',
    screenshot: 'ask-cmdr-tools.png',
    note: 'An Ask Cmdr tool line, one of the rows pictured here: what the assistant is doing or did, the note after it saying how that went, or the preview it opens. This shows other tools, so yours has its own words in the same place.',
  },
  {
    prefix: 'askCmdr.toolGroup.',
    screenshot: 'ask-cmdr-tools.png',
    note: 'The Ask Cmdr tool group header pictured here, which sums up the tool lines under it. This shows other kinds of tools.',
  },
  {
    prefix: 'askCmdr.turnStats.',
    screenshot: 'ask-cmdr-tools.png',
    note: 'The quiet stats line at the end of an Ask Cmdr reply, pictured here. Your string is another form of it.',
  },
  {
    prefix: 'askCmdr.sessions.',
    screenshot: 'ask-cmdr-sessions.png',
    note: 'The Ask Cmdr chats panel, pictured here. Your string belongs to another state of it: nothing to list, an archived chat, or a row being renamed.',
  },
  {
    prefix: 'askCmdr.renameReview.',
    screenshot: 'bulk-rename-review-all-allowed.png',
    note: 'The rename review, pictured here. Your string is one of its states: checking the names, a check that didn’t finish, or a note about a name it couldn’t keep.',
  },
  {
    prefix: 'askCmdr.renameUndo.',
    screenshot: 'ask-cmdr-chat.png',
    note: 'A line in the Ask Cmdr thread, under a reply like the one pictured here, saying how a rename went and offering to undo it. This screenshot doesn’t show that line.',
  },
  {
    prefix: 'askCmdr.error.',
    screenshot: 'ask-cmdr-chat.png',
    note: 'A line in the Ask Cmdr thread, under a message like the one pictured here, saying why the assistant couldn’t answer. This screenshot doesn’t show that line.',
  },
  {
    prefix: 'askCmdr.wakeDigest.',
    screenshot: 'ask-cmdr-chat.png',
    note: 'The fold at the top of a thread the assistant started by itself, above messages like the ones pictured here: what changed in which folders. This screenshot doesn’t show that fold.',
  },
  {
    prefix: 'askCmdr.providerNotice.',
    screenshot: 'ask-cmdr-empty.png',
    note: 'A yellow notice at the top of the Ask Cmdr panel pictured here, shown while no AI provider is ready, with a link to the settings. This screenshot doesn’t show it.',
  },
  {
    // Toasts whose other outcomes share a corner with one the run has.
    prefix: 'fileExplorer.compareDirectories.',
    screenshot: 'toast-transfer-complete.png',
    note: 'This shows a different toast in the same corner of the window; your string is what Compare directories says there when it finishes or can’t.',
  },
  {
    prefix: 'fileExplorer.folderSizes.',
    screenshot: 'toast-transfer-complete.png',
    note: 'This shows a different toast in the same corner of the window; your string is what Calculate folder sizes says there when something got in its way.',
  },
  {
    // Needs a live SMB connection to upgrade, so no surface reaches it.
    prefix: 'fileExplorer.pane.directConnectionBlockedByThisMacToast',
    screenshot: 'toast-transfer-complete.png',
    note: 'This shows a different toast in the same corner of the window; your string is what Connect directly says there when something on this Mac refused the connection, with a button that opens System Settings.',
  },
  {
    prefix: 'commands.handler.shareLink',
    screenshot: 'toast-transfer-complete.png',
    note: 'This shows a different toast in the same corner of the window; your string is what Copy share link says there, for a file in an S3 bucket.',
  },
  {
    prefix: 'fileExplorer.navigation.favoriteUnreachable.',
    screenshot: 'toast-favorite.png',
    note: 'This shows a different toast in the same corner of the window; your string is what picking a favorite says there when its place can’t be reached, with the one button that helps.',
  },
  {
    prefix: 'fileExplorer.navigation.favoriteStatus.',
    screenshot: 'favorites-menu.png',
    note: 'The favorites menu, pictured here. Your string is the line under a favorite’s path in its tooltip, for a row that can’t open right away.',
  },
  {
    prefix: 'fileExplorer.listingStalled.',
    screenshot: 'error-message-example.png',
    note: 'This shows a different screen in the same place: your string is on the screen a file pane shows while a folder’s volume has gone quiet mid-read, with a spinner, Try again, and Go back.',
  },
  {
    prefix: 'servers.refusal.addressTaken',
    screenshot: 'server-sign-in-edit.png',
    note: 'The server edit sheet, pictured here. Your string is the one line that replaces the help under its address field when another saved server already has that address.',
  },
  {
    prefix: 'servers.refusal.',
    screenshot: 'server-sign-in-refused-local-network.png',
    note: 'The server sheet, pictured here turning down a connection with a different message. Your string is a line like that one, under a field or above the sheet’s buttons.',
  },
  {
    // Edit mode reads the real saved-server store, so a gallery row can't show a saved bucket's form.
    prefix: 'servers.sheet.s3EndpointMoveHelp',
    screenshot: 'server-sign-in-edit.png',
    note: 'The server edit sheet, pictured here for another kind of server. Your string is the help under its address field when the server is an S3 bucket, saying what changing the endpoint does.',
  },
  {
    prefix: 'servers.sheet.s3',
    screenshot: 'server-sign-in-refused-s3.png',
    note: 'The server sheet for S3, pictured here. Your string belongs to a provider this shot doesn’t pick: a custom endpoint, an account ID, or the help under one of those fields.',
  },
  {
    prefix: 'settings.managed.',
    screenshot: 'settings-updates.png',
    note: 'Settings > Updates & privacy, pictured here with no policy. Your string is on a card at the top of this section, or under a locked row, while an organization manages this Mac’s Cmdr.',
  },
  {
    prefix: 'indexing.overall.',
    screenshot: 'indexing-checklist.png',
    note: 'The drive-indexing checklist, pictured here. Your string is its Overall line while it’s still working out how long is left.',
  },
]
