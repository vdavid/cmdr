/**
 * Shared formatter for the update-check status string. Used by both the Settings > Updates
 * section and the menu-triggered toast so the wording stays in sync.
 *
 * Returns `null` for the error case; callers render their own error UI (with a follow-up
 * "Send error report" link) and read `state.error` directly.
 */
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
        return `No updates found. Current version: v${prev}`
      }
      return ''
    case 'checking':
      return 'Checking…'
    case 'downloading':
      return `Update found, downloading v${next} (current: v${prev})…`
    case 'installing':
      return `Installing v${next} (current: v${prev})…`
    case 'ready':
      return `Update v${next} ready. Restart to apply.`
  }
}
