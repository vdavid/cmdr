/**
 * Module-level singleton state for the updater. Lives here (not in `updater.svelte.ts`) so toast
 * components can read it without forming an import cycle: the toast components import this module,
 * `updater.svelte.ts` imports both this module and the toast components, and the cycle stays
 * one-way.
 */

import type { BundleWriteBlocker } from '$lib/tauri-commands'
import type { ServerRequestError, UpdateCheckOutcome } from '$lib/ipc/bindings'

/** The update the last check found and staged. The backend keeps where it came from; the frontend needs the version. */
export interface UpdateInfo {
  version: string
}

/**
 * A check the organization's policy answered: updates are off, or a release is held back by the ceiling. The backend's
 * own outcome, rendered as is. ❌ Not a failure: no error copy, no report link.
 */
export type ManagedUpdateOutcome = Extract<UpdateCheckOutcome, { kind: 'updatesDisabledByPolicy' | 'heldByPolicy' }>

/**
 * Why the last check, download, or install didn't finish, as a typed value the surfaces word from the catalog
 * (`update-status-text.ts`). ❌ Never a message.
 *
 * `request` is the macOS check's typed failure, or `null` when the check failed untyped (the Tauri plugin on other
 * platforms, or a broken IPC bridge).
 */
export type UpdateFailure =
  | { phase: 'check'; request: ServerRequestError | null }
  | { phase: 'download' }
  | { phase: 'install' }

export interface UpdateState {
  status: 'idle' | 'checking' | 'downloading' | 'installing' | 'ready'
  update: UpdateInfo | null
  /** Why the last attempt didn't finish, or `null`. Cleared when a check starts. */
  failure: UpdateFailure | null
  /** The organization's answer to the last check, or `null`. Cleared when a check starts. */
  managed: ManagedUpdateOutcome | null
  /** Version the user is currently running. Set when `checking` starts. */
  previousVersion: string | null
  /** Version we're moving to. Set when an update is found. Cleared on `idle`. */
  nextVersion: string | null
}

export const updateState = $state<UpdateState>({
  status: 'idle',
  update: null,
  failure: null,
  managed: null,
  previousVersion: null,
  nextVersion: null,
})

/**
 * The "move Cmdr to Applications" nudge. `blocker` is non-null exactly while the dialog is up.
 *
 * Lives beside `updateState` rather than inside it: it isn't a phase of the update state machine,
 * it's the one thing we can do for an install that will never finish one. `+layout.svelte` mounts
 * the dialog off this, and `updater.svelte.ts` raises it.
 */
export const updateBlockerNotice = $state<{ blocker: BundleWriteBlocker | null }>({ blocker: null })
