/**
 * The names already in the directory, which the editor's red border checks a
 * typed name against.
 *
 * Reading them means paging the whole listing (500 rows per IPC round trip), so
 * a chain that crosses 20 rows of a 100k-file directory would otherwise pay for
 * 4,000 round trips to learn the same thing 20 times. This holds one read for
 * the life of a chain and follows the chain's own renames locally instead.
 *
 * ❌ It feeds the HINT only. `rename-flow`'s `decideStepFate` must never consult
 * it: a chain rewrites the directory as it runs, so a name it calls taken can be
 * perfectly free by the time the user types it, and dropping the edit on that
 * would throw away what they typed. The backend answers that question.
 * `DETAILS.md` § Chaining.
 */

import { getFileRange } from '$lib/tauri-commands'

/** Rows per IPC round trip while paging a listing. */
const BATCH_SIZE = 500

/** The listing a name list was read from, and how much of it there is to read. */
export interface ListingScope {
  listingId: string
  includeHidden: boolean
  /** The directory on disk, so a rename that lands elsewhere can't patch this list. */
  parentPath: string
  /** Rows in the listing, so the pager knows where to stop. */
  totalCount: number
}

export interface SiblingNames {
  /** Every name in the directory. Empty until the first read lands. */
  readonly names: string[]
  /** Reads the listing, or does nothing when this scope has already been read. */
  ensure(scope: ListingScope): Promise<void>
  /** A rename that landed on disk: `from` leaves the directory, `to` joins it. */
  applyRename(parentPath: string, from: string, to: string): void
  /** Forgets the listing, so the next `ensure` reads it again. */
  clear(): void
}

export function createSiblingNames(): SiblingNames {
  let scope: ListingScope | null = null
  let names: string[] = []
  /** In flight while a read is paging, so a second activation can await it rather than start its own. */
  let reading: Promise<void> | null = null
  /** Renames that landed mid-read: the snapshot being paged predates them. */
  let landedWhileReading: [string, string][] = []
  /**
   * Bumped by everything that makes a read's answer worthless (a new scope, a
   * chain ending). A read compares it on the way out, so an abandoned one can
   * never overwrite the list its successor has already filled.
   */
  let generation = 0

  function isCurrentScope(candidate: ListingScope): boolean {
    return (
      scope !== null &&
      scope.listingId === candidate.listingId &&
      scope.includeHidden === candidate.includeHidden &&
      scope.parentPath === candidate.parentPath
    )
  }

  function withRename(list: string[], from: string, to: string): string[] {
    const rest = list.filter((name) => name !== from)
    if (!rest.includes(to)) rest.push(to)
    return rest
  }

  async function read(target: ListingScope, forGeneration: number): Promise<void> {
    const loaded: string[] = []
    try {
      for (let start = 0; start < target.totalCount; start += BATCH_SIZE) {
        const count = Math.min(BATCH_SIZE, target.totalCount - start)
        const entries = await getFileRange(target.listingId, start, count, target.includeHidden)
        for (const entry of entries) loaded.push(entry.name)
      }
    } catch {
      // A listing we can't page gives no hint. The save still gets the
      // authoritative answer from the backend.
      return
    }
    if (forGeneration !== generation) return
    names = landedWhileReading.reduce((list, [from, to]) => withRename(list, from, to), loaded)
    landedWhileReading = []
  }

  return {
    get names() {
      return names
    },

    ensure(candidate: ListingScope): Promise<void> {
      if (isCurrentScope(candidate)) return reading ?? Promise.resolve()
      generation += 1
      scope = candidate
      names = []
      landedWhileReading = []
      if (candidate.listingId === '' || candidate.totalCount === 0) {
        reading = null
        return Promise.resolve()
      }
      const started = read(candidate, generation).finally(() => {
        reading = null
      })
      reading = started
      return started
    },

    applyRename(parentPath: string, from: string, to: string): void {
      if (scope === null || scope.parentPath !== parentPath) return
      names = withRename(names, from, to)
      if (reading !== null) landedWhileReading.push([from, to])
    },

    clear(): void {
      generation += 1
      scope = null
      names = []
      reading = null
      landedWhileReading = []
    },
  }
}
