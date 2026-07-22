/**
 * The `drive-index-stale` preview's event payload.
 *
 * There's no fixture DATA here on purpose: the dialog takes no props and renders
 * the name of whichever volume the event names, so the only thing a preview
 * picks is the volume — and it has to be a real one from the volume store, or
 * `volumeName()` falls back to printing the raw id. `stale-drive-preview.ts`
 * chooses it at trigger time and calls the builder, which pins the exact
 * Fresh→Stale edge the backend emits.
 */

import type { IndexFreshnessChangedEvent } from '$lib/ipc/bindings'

export const staleDriveFixtures: Record<string, ((volumeId: string) => IndexFreshnessChangedEvent) | undefined> = {
  default: (volumeId) => ({ volumeId, freshness: 'stale' }),
}
