/**
 * Base-locale (en) parity net for the file-operations dialog-chrome i18n
 * migration (copy/move/delete/new-file/new-folder dialogs).
 *
 * The dialog titles, buttons, phase labels, conflict-policy labels, scan-stat
 * nouns, and notices moved from hardcoded English into the `fileOperations.*`
 * catalog (resolved through `t()`/`<Trans>`). This is a behavior-preserving
 * MOVE: every rendered en string must be byte-identical to the pre-migration
 * copy. These goldens are the exact literals that lived in the dialog components
 * and their utils before the move; a future copy edit lands in the catalog AND
 * here together, never silently.
 *
 * The count-phrase helpers (`generateTitle`, `generateDeleteTitle`,
 * `getSymlinkNotice`, `getPathValidationError`, `formatSpaceInfo`) keep their own
 * exact-string assertions in their colocated unit tests; this file covers the
 * markup-resolved keys those tests don't reach.
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

describe('shared buttons + conflict labels (en)', () => {
  it('resolves the shared dialog buttons', () => {
    expect(tString('fileOperations.button.cancel')).toBe('Cancel')
    expect(tString('fileOperations.button.ok')).toBe('OK')
  })

  it('resolves the new-file/new-folder conflict validation copy', () => {
    expect(tString('fileOperations.shared.conflictExistsFolder')).toBe(
      'There is already a folder by this name in this folder.',
    )
    expect(tString('fileOperations.shared.conflictExistsFile')).toBe(
      'There is already a file by this name in this folder.',
    )
  })

  it('joins two count parts with " and "', () => {
    expect(t('fileOperations.shared.andJoin', { a: '3 files', b: '1 folder' })).toBe('3 files and 1 folder')
  })
})

describe('new-file and new-folder dialog chrome (en)', () => {
  it('resolves the titles, placeholders, and aria labels', () => {
    expect(tString('fileOperations.mkfile.title')).toBe('New file')
    expect(tString('fileOperations.mkfile.nameAria')).toBe('File name')
    expect(tString('fileOperations.mkfile.placeholder')).toBe('Example: notes.txt')
    expect(tString('fileOperations.mkdir.title')).toBe('New folder')
    expect(tString('fileOperations.mkdir.nameAria')).toBe('Folder name')
    expect(tString('fileOperations.mkdir.placeholder')).toBe('Example: my-project')
  })

  it('resolves the new-folder timeout warning + AI suggestion chrome', () => {
    expect(tString('fileOperations.mkdir.timeoutMessage')).toBe(
      "Couldn't confirm the folder was created. The volume may be slow, so the folder may still have been created.",
    )
    expect(tString('fileOperations.mkdir.timeoutRefresh')).toBe('Refresh listing')
    expect(tString('fileOperations.mkdir.timeoutDismiss')).toBe('Dismiss')
    expect(tString('fileOperations.mkdir.aiSuggestionsAria')).toBe('AI suggestions')
    expect(tString('fileOperations.mkdir.aiSuggestionsHeader')).toBe('AI suggestions:')
  })

  it('renders the create-in subtitle (Trans tags strip to text in en)', () => {
    // The <dir> tag wraps the folder name; the rendered text concatenation is the
    // byte-identical sentence (the component re-inserts the styled span).
    const file = t('fileOperations.mkfile.createIn', { name: 'Documents', dir: (c: unknown[]) => c.join('') })
    expect(Array.isArray(file) ? file.join('') : file).toBe('Create file in Documents')
    const folder = t('fileOperations.mkdir.createIn', { name: 'Documents', dir: (c: unknown[]) => c.join('') })
    expect(Array.isArray(folder) ? folder.join('') : folder).toBe('Create folder in Documents')
  })
})

describe('delete dialog chrome (en)', () => {
  it('resolves the from-path header and no-trash warning', () => {
    expect(t('fileOperations.delete.fromPath', { path: '~/Documents' })).toBe('From: ~/Documents')
    expect(tString('fileOperations.delete.noTrashWarningStrong')).toBe("This volume doesn't support trash.")
    expect(tString('fileOperations.delete.noTrashWarningRest')).toBe('Files will be permanently deleted.')
  })

  it('resolves the trash switch and confirm buttons', () => {
    expect(tString('fileOperations.delete.trashSwitch')).toBe('Move to trash')
    expect(tString('fileOperations.delete.confirmDelete')).toBe('Delete')
    expect(tString('fileOperations.delete.confirmMoveToTrash')).toBe('Move to trash')
  })

  it('resolves the overflow line with plural agreement', () => {
    expect(t('fileOperations.delete.overflowMore', { countText: '1', count: 1 })).toBe('... and 1 more item')
    expect(t('fileOperations.delete.overflowMore', { countText: '1,234', count: 1234 })).toBe(
      '... and 1,234 more items',
    )
  })

  it('resolves the scan-stat nouns and throughput', () => {
    expect(t('fileOperations.delete.scanFile', { count: 1 })).toBe('file')
    expect(t('fileOperations.delete.scanFile', { count: 2 })).toBe('files')
    expect(t('fileOperations.delete.scanDir', { count: 1 })).toBe('dir')
    expect(t('fileOperations.delete.scanDir', { count: 2 })).toBe('dirs')
    expect(t('fileOperations.delete.throughputFiles', { rateText: '1,200' })).toBe('1,200 files/s')
  })
})

describe('transfer dialog chrome (en)', () => {
  it('resolves the copy/move toggle, confirm buttons, and aria labels', () => {
    expect(tString('fileOperations.transferDialog.toggleCopy')).toBe('Copy')
    expect(tString('fileOperations.transferDialog.toggleMove')).toBe('Move')
    expect(tString('fileOperations.transferDialog.confirmCopy')).toBe('Copy')
    expect(tString('fileOperations.transferDialog.confirmMove')).toBe('Move')
    expect(tString('fileOperations.transferDialog.destVolumeAria')).toBe('Destination volume')
    expect(tString('fileOperations.transferDialog.destPathAria')).toBe('Destination path')
  })

  it('resolves the SMB note and checking-conflicts status', () => {
    expect(tString('fileOperations.transferDialog.smbNativeNote')).toBe(
      'This share uses the system connection. Cancellation may be delayed. Use "Connect directly" in the volume picker for faster transfers and reliable cancel.',
    )
    expect(tString('fileOperations.transferDialog.checkingConflicts')).toBe('Checking for conflicts...')
  })

  it('resolves merge info and the conflict summary with agreement', () => {
    expect(tString('fileOperations.transferDialog.mergeInfoSingle')).toBe('1 folder will merge with an existing folder')
    expect(t('fileOperations.transferDialog.mergeInfoMany', { countText: '1,234' })).toBe(
      '1,234 folders will merge with existing folders',
    )
    expect(t('fileOperations.transferDialog.conflictsSummary', { countText: '1', count: 1 })).toBe(
      '1 file already exists',
    )
    expect(t('fileOperations.transferDialog.conflictsSummary', { countText: '3', count: 3 })).toBe(
      '3 files already exist',
    )
  })

  it('resolves the conflict-policy radio labels (single vs. all)', () => {
    expect(t('fileOperations.transferDialog.policySkip', { count: 1 })).toBe('Skip')
    expect(t('fileOperations.transferDialog.policySkip', { count: 2 })).toBe('Skip all')
    expect(t('fileOperations.transferDialog.policyOverwrite', { count: 1 })).toBe('Overwrite')
    expect(t('fileOperations.transferDialog.policyOverwrite', { count: 2 })).toBe('Overwrite all')
    expect(t('fileOperations.transferDialog.policyOverwriteSmaller', { count: 1 })).toBe('Overwrite if smaller')
    expect(t('fileOperations.transferDialog.policyOverwriteSmaller', { count: 2 })).toBe('Overwrite all smaller')
    expect(t('fileOperations.transferDialog.policyOverwriteOlder', { count: 1 })).toBe('Overwrite if older')
    expect(t('fileOperations.transferDialog.policyOverwriteOlder', { count: 2 })).toBe('Overwrite all older')
    expect(t('fileOperations.transferDialog.policyStop', { count: 1 })).toBe('Ask later')
    expect(t('fileOperations.transferDialog.policyStop', { count: 2 })).toBe('Ask for each')
  })

  it('resolves the cross-type overwrite warning and the path-error sentences', () => {
    expect(tString('fileOperations.transferDialog.typeMismatchWarning')).toBe(
      'Some clashes mix a file and a folder by the same name. Overwriting will replace items of a different type, including the entire contents of a folder.',
    )
    expect(t('fileOperations.transferDialog.pathErrorSubfolder', { verb: 'copy', name: 'photos' })).toBe(
      'Can\'t copy "photos" into its own subfolder',
    )
    expect(t('fileOperations.transferDialog.pathErrorSubfolder', { verb: 'move', name: 'photos' })).toBe(
      'Can\'t move "photos" into its own subfolder',
    )
    expect(t('fileOperations.transferDialog.pathErrorAlreadyThere', { name: 'photos' })).toBe(
      '"photos" is already in this location',
    )
  })
})

describe('scan-phase body (en)', () => {
  it('resolves the from-label, scan nouns, and throughput', () => {
    expect(tString('fileOperations.scanPhase.fromLabel')).toBe('From:')
    expect(t('fileOperations.scanPhase.scanFile', { count: 1 })).toBe('file')
    expect(t('fileOperations.scanPhase.scanDir', { count: 2 })).toBe('dirs')
    expect(t('fileOperations.scanPhase.throughputFiles', { rateText: '900' })).toBe('900 files/s')
  })
})

describe('transfer progress dialog chrome (en)', () => {
  it('resolves the phase titles', () => {
    expect(tString('fileOperations.transferProgress.scanTitleCopy')).toBe('Verifying before copy...')
    expect(tString('fileOperations.transferProgress.scanTitleMove')).toBe('Verifying before move...')
    expect(tString('fileOperations.transferProgress.scanTitleDelete')).toBe('Counting items to delete...')
    expect(tString('fileOperations.transferProgress.scanTitleTrash')).toBe('Counting items to trash...')
    expect(tString('fileOperations.transferProgress.titleRollingBack')).toBe('Rolling back...')
    expect(tString('fileOperations.transferProgress.titleCancelling')).toBe('Cancelling...')
    expect(tString('fileOperations.transferProgress.titleCancellingSlow')).toBe(
      'Cancelling... (finishing USB transfers)',
    )
    expect(tString('fileOperations.transferProgress.titleConflict')).toBe('File already exists')
    expect(tString('fileOperations.transferProgress.titleFlushing')).toBe('Writing the last piece...')
  })

  it('resolves the active-phase title and stage labels per operation', () => {
    expect(t('fileOperations.transferProgress.titleActive', { gerund: 'copy' })).toBe('Copying...')
    expect(t('fileOperations.transferProgress.titleActive', { gerund: 'move' })).toBe('Moving...')
    expect(t('fileOperations.transferProgress.titleActive', { gerund: 'delete' })).toBe('Deleting...')
    expect(t('fileOperations.transferProgress.titleActive', { gerund: 'trash' })).toBe('Moving to trash...')
    expect(tString('fileOperations.transferProgress.stageScanning')).toBe('Scanning')
    expect(t('fileOperations.transferProgress.stageActive', { gerund: 'copy' })).toBe('Copying')
    expect(t('fileOperations.transferProgress.stageActive', { gerund: 'trash' })).toBe('Moving to trash')
  })

  it('resolves the conflict comparison labels and annotations', () => {
    expect(tString('fileOperations.transferProgress.existingFolderLabel')).toBe('Existing (folder):')
    expect(tString('fileOperations.transferProgress.existingFileLabel')).toBe('Existing (file):')
    expect(tString('fileOperations.transferProgress.existingLabel')).toBe('Existing:')
    expect(tString('fileOperations.transferProgress.newFolderLabel')).toBe('New (folder):')
    expect(tString('fileOperations.transferProgress.newFileLabel')).toBe('New (file):')
    expect(tString('fileOperations.transferProgress.newLabel')).toBe('New:')
    expect(tString('fileOperations.transferProgress.sizeUnknown')).toBe('(unknown)')
    expect(tString('fileOperations.transferProgress.annotationLarger')).toBe('(larger)')
    expect(tString('fileOperations.transferProgress.annotationNewer')).toBe('(newer)')
  })

  it('resolves the conflict resolution buttons and tooltips', () => {
    expect(tString('fileOperations.transferProgress.conflictSkip')).toBe('Skip')
    expect(tString('fileOperations.transferProgress.conflictSkipAll')).toBe('Skip all')
    expect(tString('fileOperations.transferProgress.conflictRename')).toBe('Rename')
    expect(tString('fileOperations.transferProgress.conflictRenameAll')).toBe('Rename all')
    expect(tString('fileOperations.transferProgress.conflictOverwrite')).toBe('Overwrite')
    expect(tString('fileOperations.transferProgress.conflictOverwriteAll')).toBe('Overwrite all')
    expect(tString('fileOperations.transferProgress.conflictOverwriteFolderWithFile')).toBe(
      'Overwrite folder with file',
    )
    expect(tString('fileOperations.transferProgress.conflictOverwriteFoldersWithFiles')).toBe(
      'Overwrite folders with files',
    )
    expect(tString('fileOperations.transferProgress.conflictOverwriteAllSmaller')).toBe('Overwrite all smaller')
    expect(tString('fileOperations.transferProgress.conflictOverwriteAllOlder')).toBe('Overwrite all older')
    expect(tString('fileOperations.transferProgress.conflictCancel')).toBe('Cancel')
    expect(tString('fileOperations.transferProgress.conflictRollback')).toBe('Rollback')
    expect(tString('fileOperations.transferProgress.rollbackUnavailableTooltip')).toBe(
      'Rollback is not available for same-volume moves',
    )
    expect(tString('fileOperations.transferProgress.rollbackTooltip')).toBe(
      'Cancel and delete any partial target files created',
    )
    expect(tString('fileOperations.transferProgress.smallerDisabledTooltip')).toBe(
      "Can't compare: target folder size is unknown.",
    )
  })

  it('resolves the progress labels, aria, ETA, and SMB note', () => {
    expect(tString('fileOperations.transferProgress.progressSize')).toBe('Size')
    expect(tString('fileOperations.transferProgress.progressItems')).toBe('Items')
    expect(tString('fileOperations.transferProgress.progressFiles')).toBe('Files')
    expect(tString('fileOperations.transferProgress.sizeProgressAria')).toBe('Size progress')
    expect(tString('fileOperations.transferProgress.fileProgressAria')).toBe('File progress')
    expect(t('fileOperations.transferProgress.etaRemaining', { duration: '2m 30s' })).toBe('~2m 30s remaining')
    expect(tString('fileOperations.transferProgress.smbNativeNote')).toBe(
      'This share uses the system connection. Cancel and rollback may be delayed.',
    )
  })

  it('renders the file-over-folder warning (Trans strong tags strip to text in en)', () => {
    const strong = (c: unknown[]) => c.join('')
    const result = t('fileOperations.transferProgress.warningFileOverFolder', { strong })
    expect(Array.isArray(result) ? result.join('') : result).toBe(
      "The target exists and is a folder. You're about to overwrite it with a file by the same name. All contents of the target folder would be deleted and replaced by the file. What to do?",
    )
  })
})

describe('error dialog chrome (en)', () => {
  it('resolves the chrome (NOT the error-message prose, owned by the errors tranche)', () => {
    expect(tString('fileOperations.errorDialog.technicalDetails')).toBe('Technical details')
    expect(tString('fileOperations.errorDialog.technicalDetailsAria')).toBe('Technical error details')
    expect(tString('fileOperations.errorDialog.retry')).toBe('Retry')
    expect(tString('fileOperations.errorDialog.close')).toBe('Close')
  })
})

describe('archive-password dialog chrome (en)', () => {
  it('resolves the titles, buttons, and field labels', () => {
    expect(tString('fileOperations.archivePassword.title')).toBe('Password needed')
    expect(tString('fileOperations.archivePassword.retryTitle')).toBe("That didn't work")
    expect(tString('fileOperations.archivePassword.inputAria')).toBe('Archive password')
    expect(tString('fileOperations.archivePassword.placeholder')).toBe('Password')
    expect(tString('fileOperations.archivePassword.unlock')).toBe('Unlock')
  })

  it('renders the prompt bodies (Trans <archive> tag strips to text in en)', () => {
    const archive = (c: unknown[]) => c.join('')
    const first = t('fileOperations.archivePassword.message', { name: 'photos.zip', archive })
    expect(Array.isArray(first) ? first.join('') : first).toBe(
      'photos.zip is password-protected. Enter its password to unlock it.',
    )
    const retry = t('fileOperations.archivePassword.retryMessage', { name: 'photos.zip', archive })
    expect(Array.isArray(retry) ? retry.join('') : retry).toBe(
      "That password didn't unlock photos.zip. Give it another go.",
    )
  })
})
