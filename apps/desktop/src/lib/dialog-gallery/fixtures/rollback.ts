/**
 * Fixtures for `rollback-confirmation`
 * (`$lib/file-operations/RollbackConfirmDialog.svelte`).
 *
 * The rollback itself lives in the `onConfirm` prop, which the gallery leaves empty,
 * so `variant` is the whole of this dialog's state: it decides the body, the cancel
 * wording, and whether the confirming button reads as destructive. All five belong in
 * the gallery because getting one of them wrong is a copy problem you can only see by
 * reading them side by side.
 */

import type { RollbackConfirmVariant } from '$lib/file-operations/reversal-wording'

export interface RollbackConfirmFixture {
  variant: RollbackConfirmVariant
}

export const rollbackConfirmFixtures: Record<string, RollbackConfirmFixture | undefined> = {
  stopAndDelete: { variant: 'stopAndDelete' },
  stopAndMoveBack: { variant: 'stopAndMoveBack' },
  undoByDeleting: { variant: 'undoByDeleting' },
  undoByMovingBack: { variant: 'undoByMovingBack' },
  undoByRenamingBack: { variant: 'undoByRenamingBack' },
}
