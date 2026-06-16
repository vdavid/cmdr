/**
 * Git-path friendly-error copy: `FriendlyGitErrorKind` → user-facing message.
 *
 * The git module classifies a git-shaped failure into a typed kind (verbatim
 * from the old Rust `git/friendly.rs`); this factory owns the words. Git copy is
 * git-domain and stays a distinct namespace from the listing reasons. All copy
 * here is fully static (no runtime-param interpolation), matching the old
 * `Markdown::literal` wrapping.
 */

import type { FriendlyErrorMessage } from './friendly-error-message'

/** Serialized camelCase from Rust `FriendlyGitErrorKind`. */
export type FriendlyGitErrorKind =
  | 'notARepo'
  | 'orphanedWorktree'
  | 'corruptRepo'
  | 'indexLocked'
  | 'permissionDenied'
  | 'bareRepo'
  | 'blobTooLarge'
  | 'shallowBoundary'
  | 'missingObject'
  | 'gitDirPermissionDenied'

const GIT_MESSAGES: Record<FriendlyGitErrorKind, FriendlyErrorMessage> = {
  notARepo: {
    title: 'No git repo here',
    message: 'Cmdr looked up the folder tree and didn\'t find a `.git` here.',
    suggestion: 'Open a folder inside a git clone to see the repo chip.',
  },
  orphanedWorktree: {
    title: 'This worktree is orphaned',
    message: "This is a linked worktree but its main repo is missing, so git can't follow the link.",
    suggestion: 'Try opening the main repo, or remove the orphan with `git worktree prune`.',
  },
  corruptRepo: {
    title: 'This repo looks damaged',
    message: 'Some of the on-disk repo data is unreadable. The folder might have been edited outside git.',
    suggestion: 'Run `git fsck` to inspect the repo. A fresh clone often clears it up.',
  },
  indexLocked: {
    title: 'Another git is mid-write',
    message: "Git's index is locked, which usually means another git command is still running.",
    suggestion: 'Wait for the running git command to finish, then navigate here again.',
  },
  permissionDenied: {
    title: "Cmdr can't read this repo",
    message: "The OS won't let Cmdr open the `.git` folder, so git info isn't available.",
    suggestion:
      'Open **System Settings > Privacy & Security > Files and Folders** and grant Cmdr access to the folder.',
  },
  bareRepo: {
    title: "Bare repos aren't supported yet",
    message: "Bare repos don't have a working tree, and the git browser is built around one.",
    suggestion: 'Clone the repo into a working directory to use the git browser.',
  },
  blobTooLarge: {
    title: "This file's too big to load from history",
    message: "Cmdr reads git blobs whole-file at a time, and this one's over the safety cap.",
    suggestion: 'Check out the file from a working tree if you want to read it.',
  },
  shallowBoundary: {
    title: 'Beyond the shallow-clone boundary',
    message: "This commit lives past the boundary of your shallow clone, so its data isn't on disk.",
    suggestion: 'Run `git fetch --unshallow` (or `--depth=N`) to bring more history into the clone.',
  },
  missingObject: {
    title: 'A git object is missing',
    message:
      'Git is looking for an object that\'s no longer in the pack files. The repo might be partially fetched or damaged.',
    suggestion: 'Try `git fetch` to repopulate the missing object, or `git fsck` to inspect the damage.',
  },
  gitDirPermissionDenied: {
    title: "Cmdr can't open the `.git` folder",
    message: 'The OS denied access to the `.git` folder, even though the working tree is readable.',
    suggestion:
      'Open **System Settings > Privacy & Security > Files and Folders** and grant Cmdr access. In Terminal, `ls -la .git` shows the current owner and mode.',
  },
}

/** Maps a git-error kind to its user-facing message (verbatim copy). */
export function getGitErrorMessage(kind: FriendlyGitErrorKind): FriendlyErrorMessage {
  return GIT_MESSAGES[kind]
}
