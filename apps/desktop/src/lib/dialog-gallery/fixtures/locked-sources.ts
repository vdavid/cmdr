/**
 * Fixtures for `locked-sources` (`#lib/file-operations/LockedSourcesDialog.svelte`).
 *
 * A move's question, before any byte moves, when some of what it moves is
 * Locked. The answers live in the `onMoveAnyway` / `onSkip` / `onCancel` props,
 * which the gallery leaves to closing the preview, so the question itself is the
 * whole state: the count picks the sentence's number and whether the name list
 * ends in "and N more".
 */

import type { OperationSnapshot, WriteLockedSourcesEvent } from '#lib/tauri-commands/index.js'

export interface LockedSourcesFixture {
  event: WriteLockedSourcesEvent
  snapshot: OperationSnapshot
}

function fixture(lockedSample: string[], lockedCount: number, destination: string): LockedSourcesFixture {
  return {
    event: { operationId: 'gallery-op', operationType: 'move', lockedCount, lockedSample },
    snapshot: {
      operationId: 'gallery-op',
      operationType: 'move',
      status: 'running',
      source: 'Contracts',
      destination,
      rollbackSupport: 'crossVolumeMove',
      reverses: null,
      error: null,
      canRetry: false,
      conflictPolicy: null,
      items: null,
    },
  }
}

const contracts = (count: number) =>
  Array.from({ length: count }, (_, i) => `/Users/me/Documents/Contracts/Contract ${String(i + 1)}.pdf`)

export const lockedSourcesFixtures: Record<string, LockedSourcesFixture | undefined> = {
  one: fixture(contracts(1), 1, 'Archive'),
  some: fixture(contracts(3), 3, 'Naspolya Archive'),
  many: fixture(contracts(5), 1_234, 'Naspolya Archive'),
}
