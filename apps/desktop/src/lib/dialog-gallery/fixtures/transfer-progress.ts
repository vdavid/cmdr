/**
 * The `transfer-progress` preview's copy per state.
 *
 * No fixture DATA here: the dialog shows a real copy, so a state only picks
 * which shape of the transfer tree that copy walks. The folders come from the
 * dev-only fixture command (`TransferPreviewFixtures`), the side that creates
 * them on disk and paces copies out of them, and `transfer-progress-preview.ts`
 * opens the dialog that starts the copy.
 */

import type { TransferPreviewFixtures } from '#lib/ipc/bindings.js'

/** What one state copies, and where to: the shipping dialog's own start props. */
export interface TransferProgressRun {
  sourcePaths: string[]
  sourceFolderPath: string
  destinationPath: string
}

function copyOf(fixtures: TransferPreviewFixtures, name: string): TransferProgressRun {
  return {
    // eslint-disable-next-line cmdr/no-raw-path-arithmetic -- a gallery fixture's own POSIX path
    sourcePaths: [`${fixtures.fromDir}/${name}`],
    sourceFolderPath: fixtures.fromDir,
    destinationPath: fixtures.toDir,
  }
}

export const transferProgressFixtures: Record<
  string,
  ((fixtures: TransferPreviewFixtures) => TransferProgressRun) | undefined
> = {
  folders: (fixtures) => copyOf(fixtures, fixtures.folders),
  'compact-chains': (fixtures) => copyOf(fixtures, fixtures.compactChains),
  flat: (fixtures) => copyOf(fixtures, fixtures.flat),
}
