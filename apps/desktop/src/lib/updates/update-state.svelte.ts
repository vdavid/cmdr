/**
 * Module-level singleton state for the updater. Lives here (not in `updater.svelte.ts`) so toast
 * components can read it without forming an import cycle: the toast components import this module,
 * `updater.svelte.ts` imports both this module and the toast components, and the cycle stays
 * one-way.
 */

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
