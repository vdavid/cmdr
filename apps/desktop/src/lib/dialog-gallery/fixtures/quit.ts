/**
 * Fixtures for the `quit-confirmation` dialog (`$lib/quit/QuitConfirmationDialog.svelte`).
 *
 * Raw copy on purpose: this module is dev-only and sits outside the i18n-enforced
 * areas, so fixture strings never reach the message catalog.
 *
 * The states cover the three things that break this panel's layout: a long
 * filename with nowhere to wrap, more operations than the list's height cap, and
 * the last second of the countdown (where the sentence goes singular).
 */

import type { OperationSnapshot } from '$lib/tauri-commands'

export interface QuitFixture {
  operations: OperationSnapshot[]
  secondsLeft: number
}

function operation(
  operationId: string,
  operationType: OperationSnapshot['operationType'],
  source: string | null,
  destination: string | null,
): OperationSnapshot {
  return {
    operationId,
    operationType,
    status: 'running',
    source,
    destination,
    supportsRollback: operationType === 'copy' || operationType === 'move',
    error: null,
  }
}

export const quitFixtures: Record<string, QuitFixture | undefined> = {
  'one-copy': {
    operations: [operation('op-1', 'copy', 'Holiday.mov', 'Backup')],
    secondsLeft: 15,
  },
  'several-operations': {
    operations: [
      operation('op-1', 'copy', 'Holiday.mov', 'Backup'),
      operation('op-2', 'move', 'Invoices 2026', 'Archive'),
      operation('op-3', 'delete', 'old-renders (1,284 items)', null),
      operation('op-4', 'trash', 'Screenshot 2026-08-10.png', null),
      operation('op-5', 'archive_edit', 'press-kit.zip', null),
    ],
    secondsLeft: 9,
  },
  // No spaces to wrap on, and a destination just as bad: both cells have to
  // ellipsize rather than widen the panel.
  'long-names': {
    operations: [
      operation(
        'op-1',
        'copy',
        '2026-07-14_stockholm-archipelago-sunrise-session_DSC09241_edited_final_v3_reallyfinal.arw',
        'naspolya-media-photos-2026-07-summer-archive-raw-originals',
      ),
    ],
    secondsLeft: 12,
  },
  // The singular branch of the countdown sentence, and the moment the panel is
  // about to go away on its own.
  'last-second': {
    operations: [operation('op-1', 'copy', 'Holiday.mov', 'Backup')],
    secondsLeft: 1,
  },
}
