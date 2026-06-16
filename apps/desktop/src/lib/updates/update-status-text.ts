/**
 * Shared formatter for the update-check status string. Used by both the Settings > Updates
 * section and the menu-triggered toast so the wording stays in sync.
 *
 * Returns `null` for the error case; callers render their own error UI (with a follow-up
 * "Send error report" link) and read `state.error` directly.
 */
import { tString } from '$lib/intl/messages.svelte'

export interface UpdateStatusReadable {
  status: 'idle' | 'checking' | 'downloading' | 'installing' | 'ready'
  error: string | null
  previousVersion: string | null
  nextVersion: string | null
}

export function formatUpdateStatus(state: UpdateStatusReadable): string | null {
  if (state.error !== null) return null

  const prev = state.previousVersion ?? '?'
  const next = state.nextVersion ?? '?'

  switch (state.status) {
    case 'idle':
      // Two sub-cases share idle. If we just finished a successful check (we have a previousVersion
      // and no nextVersion), say "no updates found". Before any check has run, say nothing.
      if (state.previousVersion !== null && state.nextVersion === null) {
        return tString('updates.status.noUpdates', { version: prev })
      }
      return ''
    case 'checking':
      return tString('updates.status.checking')
    case 'downloading':
      return tString('updates.status.downloading', { next, prev })
    case 'installing':
      return tString('updates.status.installing', { next, prev })
    case 'ready':
      return tString('updates.status.ready', { next })
  }
}
