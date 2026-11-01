/**
 * The words around an expanded tool line's preview: how much of the list the model got, how much
 * of that shows here, and why a path came back without an answer. The backend sends counts and
 * typed statuses only; every sentence is ours.
 */

import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { ToolPreview, ToolPreviewRowStatus } from '#lib/ipc/bindings.js'

/** The notes under a preview's rows, each `null` when it has nothing to say. */
export interface PreviewNotes {
  /** The model got only part of the list: "Showed the model 200 of 4,812". */
  given: string | null
  /** This preview shows only the first rows of what the model got. */
  firstRows: string | null
  /** The result is facts, not a list, so only Copy has it. */
  noRows: string | null
}

/** What to say under a preview. ❌ Never more than the result carried: the counts are its own. */
export function previewNotes(preview: ToolPreview): PreviewNotes {
  const given = preview.rowsGivenToModel
  const cut = given !== null && preview.total !== null && (preview.lowerBound || preview.total > given)
  const shown = preview.rows.length
  const showsAll = preview.problem !== null || preview.setAside
  return {
    given: cut
      ? tString(preview.lowerBound ? 'askCmdr.tool.preview.givenSoFar' : 'askCmdr.tool.preview.given', {
          givenText: formatInteger(given),
          totalText: formatInteger(preview.total ?? 0),
        })
      : null,
    firstRows:
      shown > 0 && given !== null && shown < given
        ? tString('askCmdr.tool.preview.firstRows', { count: shown, countText: formatInteger(shown) })
        : null,
    noRows: shown === 0 && given === null && !showsAll ? tString('askCmdr.tool.preview.noRows') : null,
  }
}

/** Why a path came back without an answer. A `Record` over every status: a new one is a compile
 * error here. */
const STATUS_KEYS: Record<ToolPreviewRowStatus, MessageKey> = {
  missing: 'askCmdr.tool.preview.status.missing',
  unreadable: 'askCmdr.tool.preview.status.unreadable',
  unreachable: 'askCmdr.tool.preview.status.unreachable',
  unsupportedVolume: 'askCmdr.tool.preview.status.unsupportedVolume',
  noVolumeAtPath: 'askCmdr.tool.preview.status.noVolumeAtPath',
  notIndexed: 'askCmdr.tool.preview.status.notIndexed',
  protected: 'askCmdr.tool.preview.status.protected',
}

export function previewStatusText(status: ToolPreviewRowStatus): string {
  return tString(STATUS_KEYS[status])
}
