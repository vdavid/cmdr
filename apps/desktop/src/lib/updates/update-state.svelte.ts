/**
 * Module-level singleton state for the updater. Lives here (not in `updater.svelte.ts`) so toast
 * components can read it without forming an import cycle: the toast components import this module,
 * `updater.svelte.ts` imports both this module and the toast components, and the cycle stays
 * one-way.
 */

import type { BundleWriteBlocker } from '$lib/tauri-commands'

/** Metadata returned by the `check_for_update` Tauri command */
export interface UpdateInfo {
  version: string
  url: string
  signature: string
}

export interface UpdateState {
  status: 'idle' | 'checking' | 'downloading' | 'installing' | 'ready'
  update: UpdateInfo | null
  error: string | null
  /** Version the user is currently running. Set when `checking` starts. */
  previousVersion: string | null
  /** Version we're moving to. Set when an update is found. Cleared on `idle`. */
  nextVersion: string | null
}

export const updateState = $state<UpdateState>({
  status: 'idle',
  update: null,
  error: null,
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
