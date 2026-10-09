/**
 * How a preview row's status reads in the list: problem rows get a glyph of their own
 * (a short label for its accessible name, the full reason for its tooltip), and ready
 * and unchanged rows stay quiet, with words for screen readers only. Pure: it names
 * message keys, and the sheet words them.
 */

import type { MessageKey } from '$lib/intl/keys.gen'
import type { RowStatus } from '$lib/ipc/bindings'
import type { IconName } from '$lib/ui/icons/icon-map'

export interface StatusMessage {
  key: MessageKey
  params?: Record<string, string>
}

export interface RowStatusView {
  /** Anything but ready or unchanged: the row won't be renamed, and says why. */
  isProblem: boolean
  /** The problem's glyph, one per kind; `null` for ready and unchanged rows. */
  glyph: IconName | null
  /** A couple of words: the glyph's accessible name, or a quiet row's screen-reader text. */
  label: StatusMessage
  /** The full reason, for the glyph's tooltip. `null` on a quiet row. */
  reason: StatusMessage | null
}

const INVALID_REASONS = {
  empty: 'multiRename.reason.empty',
  tooLong: 'multiRename.reason.tooLong',
  reserved: 'multiRename.reason.reserved',
} as const satisfies Record<string, MessageKey>

export function rowStatusView(status: RowStatus): RowStatusView {
  const problem = (glyph: IconName, label: StatusMessage, reason: StatusMessage): RowStatusView => ({
    isProblem: true,
    glyph,
    label,
    reason,
  })
  switch (status.type) {
    case 'ready':
      return { isProblem: false, glyph: null, label: { key: 'multiRename.status.ready' }, reason: null }
    case 'unchanged':
      return { isProblem: false, glyph: null, label: { key: 'multiRename.status.unchanged' }, reason: null }
    case 'duplicate':
      return problem('copy', { key: 'multiRename.status.duplicate' }, { key: 'multiRename.reason.duplicate' })
    case 'targetExists':
      return problem('circle-x', { key: 'multiRename.status.targetExists' }, { key: 'multiRename.reason.targetExists' })
    case 'missing':
      return problem('circle-dashed', { key: 'multiRename.status.missing' }, { key: 'multiRename.reason.missing' })
    case 'invalidName': {
      const reason = status.reason
      if (reason.type === 'disallowedCharacter') {
        const params = { character: reason.character }
        return problem(
          'circle-slash',
          { key: 'multiRename.status.disallowedCharacter', params },
          { key: 'multiRename.reason.disallowedCharacter', params },
        )
      }
      return problem('circle-slash', { key: 'multiRename.status.invalidName' }, { key: INVALID_REASONS[reason.type] })
    }
  }
}
