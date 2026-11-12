/**
 * What the seed generator (`platform-copy-seed.ts`) knows beyond
 * `copy-proposal.json`: the gates, the rows resolved at seeding time (the 14
 * `.mac`/`.other` pairs and the two accepted macOS changes), the rows the
 * proposal never listed, and the Windows register of each Trash and Quit key.
 * Every entry comes from `docs/specs/platform-copy-plan.md` § "Copy resolution"
 * or `docs/notes/windows-port/copy-row-verdicts.md`; nothing here is a new choice.
 */

import type { BranchName, CopyRegister, GatedKeys, LintException } from './platform-copy-rows.ts'

/** Surfaces hidden off macOS (`copy-row-verdicts.md` § Gates): their keys stay macOS text. */
export const GATED_KEYS: readonly GatedKeys[] = [
  { keys: 'fileExplorer.quickLookHint.*', capability: 'quickLook' },
  { keys: 'settings.fileExplorer.suppressQuickLookHint.*', capability: 'quickLook' },
  { keys: 'menu.file.quickLook', capability: 'quickLook (`macos_only` in bar.rs)' },
  { keys: 'menu.file.getInfo', capability: 'getInfo (`macos_only` in bar.rs)' },
  { keys: 'settings.network.localNetworkAccessLabel', capability: 'localNetworkPermission' },
  { keys: 'settings.network.permissionIntroPrefix', capability: 'localNetworkPermission' },
  { keys: 'settings.network.permissionIntroConnectLink', capability: 'localNetworkPermission' },
  { keys: 'settings.network.permissionIntroSuffix', capability: 'localNetworkPermission' },
  { keys: 'settings.network.manageAnytimePrefix', capability: 'localNetworkPermission' },
  { keys: 'settings.network.permissionPath', capability: 'localNetworkPermission' },
  { keys: 'settings.network.permissionWithout', capability: 'localNetworkPermission' },
  { keys: 'settings.listing.showTags.*', capability: 'colorTags' },
  { keys: 'settings.ai.tooltipLocal', capability: 'localAi' },
  { keys: 'settings.ai.tooltipLocalDisabled', capability: 'localAi' },
  { keys: 'onboarding.stepAi.localTooltip', capability: 'localAi' },
  { keys: 'ai.local.*', capability: 'localAi' },
  { keys: 'settings.advanced.showSafeSaveFiles.*', capability: 'safeSaveFiles' },
  { keys: 'settings.archives.card.bundles', capability: 'appBundles' },
  { keys: 'settings.archives.bundle.*', capability: 'appBundles' },
  { keys: 'settings.behavior.archiveEnter.bundle.keywords', capability: 'appBundles' },
  { keys: 'settings.advanced.exitFullScreenOnEscape.*', capability: 'fullScreen' },
  { keys: 'settings.advanced.exitFullScreenOnEscapeHintShown.*', capability: 'fullScreen' },
  { keys: 'main.escapeFullScreenHint.*', capability: 'fullScreen' },
  { keys: 'onboarding.stepBeta.checklist.starNote', capability: 'homebrewCask' },
  { keys: 'main.dockPinNudge.*', capability: 'dockPin' },
  { keys: 'ui.toolAsk.openBundle.*', capability: 'none: the macOS launch path is its only caller' },
  {
    keys: 'fileExplorer.navigation.driveIndex.tooltipCoalesced*',
    capability: 'none: only FSEvents reports a coalesced overflow',
  },
]

/** A row resolved at seeding time. `key` is the survivor for a pair. */
export interface ResolvedSeed {
  key: string
  collapseFrom?: { mac: string; other: string }
  branches: Record<BranchName, string>
  sources: Partial<Record<BranchName | 'row', string>>
  written?: BranchName[]
  openBranches?: ('windows' | 'other')[]
  lintExceptions?: LintException[]
  macosChanged?: boolean
  note?: string
}

const OLD_OTHER = 'plan § Branch-writing rules 1: the old `.other` English, byte for byte'
/** The transfer dialog shows an `errors.write.*` suggestion as plain text, so a label is quoted, never bold. */
const QUOTED_LABEL =
  'proposal, with the label in curly quotes where it had markdown bold: the transfer dialog shows an `errors.write.*` suggestion as plain text'

const KEYRING_BODY = 'Unlock your keyring (Passwords / Keyrings app) and try again.'

/**
 * The 14 pairs (survivors per the plan's milestone 7) and the two accepted macOS
 * changes. `macos` is today's `.mac` English and `other` today's `.other`, unless
 * `sources` says otherwise.
 */
export const RESOLVED_SEEDS: readonly ResolvedSeed[] = [
  {
    key: 'commands.fileShowInFileManager.label',
    collapseFrom: { mac: 'commands.fileShowInFinder.mac.label', other: 'commands.fileShowInFinder.other.label' },
    branches: { macos: 'Show in Finder', windows: 'Show in File Explorer', other: 'Show in file manager' },
    sources: { windows: 'proposal', other: OLD_OTHER },
  },
  {
    key: 'commands.fileGetInfo.label',
    collapseFrom: { mac: 'commands.fileGetInfo.mac.label', other: 'commands.fileGetInfo.other.label' },
    branches: { macos: 'Get info', windows: 'Properties', other: 'File properties' },
    sources: { windows: 'proposal (evidence: `get-info` confirmed)', other: OLD_OTHER },
    note: 'The proposal’s Linux “Properties” is polish, not a platform need, so the old text and its translations stay. The command is hidden off macOS (`needsCapability: getInfo`).',
  },
  {
    key: 'commands.fileQuickLook.label',
    collapseFrom: { mac: 'commands.fileQuickLook.mac.label', other: 'commands.fileQuickLook.other.label' },
    branches: { macos: 'Quick look', windows: 'Preview', other: 'Preview' },
    sources: { windows: 'proposal', other: OLD_OTHER },
    note: 'Hidden off macOS (`needsCapability: quickLook`).',
  },
  {
    key: 'errors.write.fileLocked.suggestion',
    collapseFrom: { mac: 'errors.write.fileLocked.suggestion.mac', other: 'errors.write.fileLocked.suggestion.other' },
    branches: {
      macos: 'Unlock it in Finder (Get Info > uncheck Locked) and try again.',
      windows: 'The file may be read-only. In File Explorer, open its Properties, clear “Read-only”, and try again.',
      other: 'The file may be protected. Check its permissions (e.g. via chmod or your file manager) and try again.',
    },
    sources: { windows: QUOTED_LABEL, other: OLD_OTHER },
    written: ['windows'],
  },
  {
    key: 'errors.write.permissionDenied.suggestion.delete',
    collapseFrom: {
      mac: 'errors.write.permissionDenied.suggestion.deleteMac',
      other: 'errors.write.permissionDenied.suggestion.deleteOther',
    },
    branches: {
      macos:
        'Check that you have write access to the parent folder. The file may be locked. Unlock it in Finder (Get Info > uncheck Locked) and try again.',
      windows:
        'Check that you have write access to the parent folder. The file may be read-only: in File Explorer, open its Properties, clear “Read-only”, and try again.',
      other:
        'Check that you have write access to the parent folder. The file may be protected. Check its permissions (e.g. via chmod or your file manager) and try again.',
    },
    sources: { windows: QUOTED_LABEL, other: OLD_OTHER },
    written: ['windows'],
  },
  {
    key: 'errors.write.permissionDenied.suggestion.needsAdmin',
    collapseFrom: {
      mac: 'errors.write.permissionDenied.suggestion.needsAdminMac',
      other: 'errors.write.permissionDenied.suggestion.needsAdminOther',
    },
    branches: {
      macos:
        'Changing it needs administrator rights. To give your own account access, select the folder in Finder, choose Get Info, and look under Sharing & Permissions.',
      windows:
        'Changing it needs administrator rights. To give your own account access, open the folder’s Properties in File Explorer and look under “Security”.',
      other:
        'Changing it needs administrator rights. To give your own account access, adjust the folder’s permissions (for example with chmod) and try again.',
    },
    sources: { windows: QUOTED_LABEL, other: OLD_OTHER },
    written: ['windows'],
  },
  {
    key: 'errors.write.permissionDenied.suggestion.systemProtected',
    collapseFrom: {
      mac: 'errors.write.permissionDenied.suggestion.systemProtectedMac',
      other: 'errors.write.permissionDenied.suggestion.systemProtectedOther',
    },
    branches: {
      macos:
        'macOS protects this one itself, so administrator rights won’t change it. The item may be locked: select it in Finder, choose Get Info, and uncheck Locked.',
      windows:
        'Windows protects this one itself, so administrator rights alone won’t change it. System files usually belong to Windows and are best left alone.',
      other:
        'The system protects this one itself, so administrator rights won’t change it. The item may carry a protection flag that has to be cleared first.',
    },
    sources: { windows: 'proposal', other: OLD_OTHER },
  },
  {
    key: 'settings.appearance.appColorHintLink',
    collapseFrom: {
      mac: 'settings.appearance.appColorHintLinkMac',
      other: 'settings.appearance.appColorHintLinkOther',
    },
    branches: {
      macos: '{systemSettings} > {appearance}',
      windows: 'Settings > Personalization > Colors',
      other: 'your system appearance settings',
    },
    sources: { windows: 'proposal (evidence: `system-settings`, high)', other: OLD_OTHER },
  },
  {
    key: 'ai.secretError.title',
    collapseFrom: { mac: 'ai.secretError.keychainTitle', other: 'ai.secretError.keyringTitle' },
    branches: {
      macos:
        '{op, select, save {Couldn’t save your API key: macOS Keychain denied access} other {Couldn’t read your saved API key: macOS Keychain denied access}}',
      windows:
        '{op, select, save {Couldn’t save your API key: Credential Manager denied access} other {Couldn’t read your saved API key: Credential Manager denied access}}',
      other:
        '{op, select, save {Couldn’t save your API key: your system keyring denied access} other {Couldn’t read your saved API key: your system keyring denied access}}',
    },
    sources: {
      windows: 'evidence § Corrections: “Credential Manager”, without the proposal’s “Windows”',
      other: OLD_OTHER,
    },
  },
  {
    key: 'ai.secretError.body',
    collapseFrom: { mac: 'ai.secretError.keychainBody', other: 'ai.secretError.keyringBody' },
    branches: {
      macos:
        '{op, select, save {Open Keychain Access and check the “Cmdr” entry, or delete it and try saving again.} other {Open Keychain Access, find the “Cmdr” entry, grant access, or delete it and re-enter your key.}}',
      windows:
        '{op, select, save {Open Credential Manager, check the “Cmdr” entry under Windows Credentials, or remove it and try saving again.} other {Open Credential Manager, find the “Cmdr” entry under Windows Credentials, or remove it and enter your key again.}}',
      other: KEYRING_BODY,
    },
    sources: { windows: 'proposal', other: OLD_OTHER },
    note: 'The proposal’s Linux text names GNOME and KDE apps, which waits for the desktop value (task 59).',
  },
  {
    key: 'ai.secretError.removeBody',
    collapseFrom: { mac: 'ai.secretError.removeKeychainBody', other: 'ai.secretError.keyringBody' },
    branches: {
      macos: 'Open Keychain Access, find the “Cmdr” entry, and delete it there.',
      windows: 'Open Credential Manager, find the “Cmdr” entry under Windows Credentials, and remove it there.',
      other: KEYRING_BODY,
    },
    sources: { windows: 'proposal', other: OLD_OTHER },
    note: '`ai.secretError.keyringBody` feeds this survivor and `ai.secretError.body`.',
  },
  {
    key: 'mtp.connectedToast.body',
    collapseFrom: { mac: 'mtp.connectedToast.bodyMac', other: 'mtp.connectedToast.bodyOther' },
    branches: {
      macos:
        'Cmdr paused the macOS camera daemon (ptpcamerad) to access this device. To use it in another app, disable MTP support in settings.',
      windows: '',
      other: 'To use this device in another app, disable MTP support in settings.',
    },
    sources: {
      windows:
        'copy-row-verdicts.md: no body on Windows. WPD shares the device with File Explorer, so nothing is paused and no other app is locked out. The branch is empty, and MtpConnectedToastContent.svelte renders no body line for an empty text',
      other: `${OLD_OTHER} (the proposal’s “turn off … Settings” is polish)`,
    },
  },
  {
    key: 'main.upgradeNudge',
    collapseFrom: { mac: 'main.upgradeNudge.mac', other: 'main.upgradeNudge.other' },
    branches: {
      macos: 'We’ve added new onboarding options. Open Cmdr > Onboarding… to review them.',
      windows: 'We’ve added new onboarding options. Open the command palette and run Onboarding… to review them.',
      other: 'We’ve added new onboarding options. Open the command palette and run Onboarding… to review them.',
    },
    sources: {
      windows: 'proposal',
      other: `${OLD_OTHER}. The proposal’s Linux text repeats the macOS menu path, a slip: its own note says Linux has no Onboarding menu entry`,
    },
  },
  {
    key: 'menu.file.showInFileManager',
    collapseFrom: { mac: 'menu.file.showInFinder', other: 'menu.file.showInFileManager' },
    branches: { macos: 'Show in Finder', windows: 'Show in File Explorer', other: 'Show in file manager' },
    sources: { windows: 'proposal', other: OLD_OTHER },
  },
  {
    key: 'ai.secretError.genericBody',
    branches: {
      macos: 'Check that your Keychain is available, then try again. If it persists, restart the app.',
      windows: 'Check that Credential Manager is available, then try again. If it keeps happening, restart Cmdr.',
      other: 'Check that your system keyring is available, then try again. If it persists, restart the app.',
    },
    sources: {
      macos: 'decisions: the accepted change, “keyring” → “Keychain”',
      windows: 'verdicts: proposal',
      other: 'plan § Branch-writing rules 1: the old English, byte for byte',
    },
    macosChanged: true,
  },
  {
    key: 'goToPath.dialog.inputPlaceholder',
    branches: {
      macos: 'Type or paste a path, for example ~/Documents',
      windows: 'Type or paste a path, for example ~/Documents',
      other: 'Type or paste a path, for example ~/Documents',
    },
    sources: {
      macos: 'decisions: the accepted change, “e.g.” → “for example”',
      windows: 'verdicts: open, keep the macOS text until Windows path parsing lands',
      other: 'verdicts',
    },
    macosChanged: true,
    openBranches: ['windows'],
    note: 'Windows is open (`copy-decisions.md`): all three read the same text, so this stays a plain value.',
  },
]

/** A row the proposal's `strings` never listed, seeded `pending` (or `none`) from today's English. */
export interface AddedSeed {
  key: string
  status: 'pending' | 'none'
  note: string
}

const USER_FOLDER =
  'plan § Rows beyond the proposal: the decisions overturn “reviewed, no change”. Windows says “user folder”.'
const PERMANENT =
  'plan § Delete labels vs Recycle Bin sentences: the permanent sibling says “permanently” on Windows, where the trash label reads “Delete” on the same surface. Flag on the review page.'
const QUIT_NARRATION =
  'plan § Exit vs close: narration, Windows “close”; Linux “quit”, equal to macOS, so a `{windows, other}` block.'

export const ADDED_SEEDS: readonly AddedSeed[] = [
  { key: 'errors.write.homeFolder.title', status: 'pending', note: USER_FOLDER },
  { key: 'errors.write.homeFolder.message.isHome', status: 'pending', note: USER_FOLDER },
  { key: 'errors.write.homeFolder.message.holdsHome', status: 'pending', note: USER_FOLDER },
  { key: 'errors.write.homeFolderMove.title', status: 'pending', note: USER_FOLDER },
  { key: 'errors.write.homeFolderRename.title', status: 'pending', note: USER_FOLDER },
  { key: 'commands.navGoHome.label', status: 'pending', note: `${USER_FOLDER} Windows: “Go to user folder”.` },
  { key: 'indexing.phase.home', status: 'pending', note: USER_FOLDER },
  { key: 'stateRecovery.appStatus.title', status: 'pending', note: USER_FOLDER },
  { key: 'fileExplorer.errorPane.goHome', status: 'pending', note: USER_FOLDER },
  { key: 'fileExplorer.unreachable.openHome', status: 'pending', note: USER_FOLDER },
  {
    key: 'settings.fileOperations.skipConfirmation.keywords',
    status: 'pending',
    note: 'plan § Rows beyond the proposal: Windows adds “recycle bin”.',
  },
  {
    key: 'fileOperations.delete.confirmDelete',
    status: 'pending',
    note: `${PERMANENT} Windows: “Delete permanently”.`,
  },
  { key: 'suggestedOps.action.delete', status: 'pending', note: `${PERMANENT} Windows: “Delete permanently”.` },
  { key: 'suggestedOps.approve.delete', status: 'pending', note: PERMANENT },
  { key: 'askCmdr.tool.quit.doing', status: 'pending', note: `${QUIT_NARRATION} Windows: “Closing Cmdr”.` },
  { key: 'main.quit.countdown', status: 'pending', note: `${QUIT_NARRATION} Windows: “Closing in …”.` },
  {
    key: 'main.quit.countdownAria',
    status: 'pending',
    note: `${QUIT_NARRATION} Windows: “…until Cmdr closes on its own”.`,
  },
  {
    key: 'errors.git.permissionDenied.suggestion',
    status: 'pending',
    note: 'plan § Values the proposal never scanned: sends Windows and Linux users to `{system_settings} > {privacy_and_security}`; reachable wherever git reports EACCES, so it gets Windows and Linux branches.',
  },
  {
    key: 'errors.listing.tccRestricted.suggestion',
    status: 'none',
    note: 'plan § Values the proposal never scanned: macOS-only (TCC), skip.',
  },
]

/** Notes the plan or the verdicts attach to a proposal row, keyed by the row's catalog key. */
export const ROW_NOTES: Readonly<Record<string, string>> = {
  'errors.provider.appBased.serious':
    'verdicts: write now (OneDrive and Dropbox detection is coming). Windows “Close and reopen {app}”; Linux keeps the macOS text.',
  'fileOperations.sourcesGone.continue': `Its \`delete\` arm is a permanent sibling. ${PERMANENT}`,
  'ai.managed.localOnlyUnsupported':
    'Reachable off macOS now that the Local AI option is gated there (an MDM “local only” policy).',
  'fileOperations.validation.pathNotAbsolute': 'Windows names drive letters and UNC paths (plan D1).',
}

/** Proposal `none` rows a verdict promotes to work: seeded `pending` instead. */
export const PROMOTED_KEYS: ReadonlySet<string> = new Set(['errors.provider.appBased.serious'])

/** The Windows register per Trash and Quit key (plan § "Conflicts between the docs, settled"). */
export const REGISTERS: Readonly<Record<string, CopyRegister>> = {
  // Quit: the action.
  'commands.appQuit.label': 'action',
  'settings.dataWipe.quit': 'action',
  'settings.dataWipe.confirm': 'action',
  'main.quit.title': 'action',
  'main.quit.quitNow': 'action',
  'ui.toolAsk.quit.title': 'action',
  // Quit: narration.
  'settings.dataWipe.description': 'narration',
  'settings.dataWipe.intro': 'narration',
  'settings.dataWipe.introWithSize': 'narration',
  'askCmdr.decision.interrupted': 'narration',
  'askCmdr.tool.quit.done': 'narration',
  'askCmdr.tool.quit.tried': 'narration',
  'askCmdr.tool.quit.doing': 'narration',
  'crashReporter.dialog.body.ended': 'narration',
  'operationLog.rollback.interruptedNotice': 'narration',
  'main.quit.countdown': 'narration',
  'main.quit.countdownAria': 'narration',
  // Trash: the label says "Delete".
  'fileOperations.delete.confirmMoveToTrash': 'label',
  'suggestedOps.action.trash': 'label',
  'suggestedOps.approve.trash': 'label',
  'fileOperations.sourcesGone.continue': 'label',
  'commands.editUndoTrash.label': 'label',
  'commands.editUndoTrash.one': 'label',
  'commands.editUndoTrash.many': 'label',
  'menu.edit.undoTrash': 'label',
  'menu.edit.undoTrashOne': 'label',
  'menu.edit.undoTrashMany': 'label',
  'fileExplorer.renameConflict.overwriteTrash': 'label',
  'fileExplorer.renameConflict.overwriteTrashFolder': 'label',
  'settings.fileOperations.skipConfirmation.label': 'label',
  // The permanent siblings are labels too.
  'fileOperations.delete.confirmDelete': 'label',
  'suggestedOps.action.delete': 'label',
  'suggestedOps.approve.delete': 'label',
  // Trash: these name the place, so they follow the sentence register.
  'fileOperations.delete.trashSwitch': 'sentence',
  'fileOperations.transferDialog.titleVerbOnly': 'sentence',
  'fileOperations.transferDialog.titleWithCounts': 'sentence',
  'commands.fileGoToTrash.label': 'sentence',
  'fileOperations.trash.goToTrashAction': 'sentence',
}
