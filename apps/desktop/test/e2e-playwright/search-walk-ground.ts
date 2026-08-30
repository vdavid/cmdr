/**
 * Ground for the live-walk specs: a folder the index doesn't cover, on a drive
 * the index doesn't hold.
 *
 * **The E2E instance indexes its fixture tree.** `CMDR_E2E_START_PATH` narrows the
 * boot scan to the fixture root (`scanner/exclusions.rs::e2e_allowlist_path`), and
 * that scan finishes milliseconds after launch — so every folder a spec can reach
 * is COVERED, and a search over it reads the index and never walks. A spec that
 * asserts "rows appeared" against that instance is satisfied by an index read and
 * proves nothing about walking.
 *
 * So a live-walk spec has to do two things before it presses Enter:
 *
 * 1. **Take the index away** ([`makeLocalVolumeUnindexed`]), through the same two
 *    per-drive actions a user has: turn indexing off for the drive, then forget its
 *    index. What's left is the state this whole feature exists for — a drive Cmdr
 *    holds nothing about — and rows can then only come from a walk. It also pins
 *    Decision 13: neither indexing switch gates a walk somebody ASKED for.
 * 2. **Give the walk enough ground to still be walking** ([`createWalkGround`]).
 *    The fixture tree is nine directories, which a walk finishes in under a second
 *    even throttled. This builds a CHAIN instead: each directory holds exactly one
 *    subdirectory, so no amount of walker parallelism can overlap two reads and the
 *    walk takes at least `depth × CMDR_E2E_WALK_THROTTLE_MS` no matter how fast the
 *    machine is. Measured at ~7 s for the default depth on an M-series Mac.
 *
 * ❌ Don't "simplify" a spec by dropping step 1: the assertions all still pass
 * against an index-served run, silently, which is how these two specs came to prove
 * nothing in the first place.
 */

import fs from 'fs'
import path from 'path'

import { expect } from './fixtures.js'
import { getFixtureRoot } from './helpers.js'
import { mcpCall, mcpReadResource } from '../e2e-shared/mcp-client.js'

/** The local disk's volume id, the one an E2E instance indexes. */
const LOCAL_VOLUME_ID = 'root'

/**
 * How many directories deep the chain goes.
 *
 * The walk is serial through it, so this times the throttle the checker sets
 * (`CMDR_E2E_WALK_THROTTLE_MS`) is the FLOOR on how long the walk runs — the number
 * a spec's mid-walk assertions get to happen inside. Raise it if a lane's assertions
 * start racing the walk's end; every level costs one directory and one file.
 *
 * ⚠️ Stay UNDER the dialog's 30-row cap (`query-filter-state.svelte.ts`): one match
 * per level, so a deeper chain makes the run report itself capped and a spec that
 * expects the ordinary "N of N results" line reads the cap sentence instead.
 */
const WALK_GROUND_DEPTH = 24

/** Where the chain lives: a sibling of `left/` and `right/`, so neither is disturbed. */
export function walkGroundPath(): string {
  return path.join(getFixtureRoot(), 'walk-ground')
}

/**
 * Builds the chain, replacing any leftover from an earlier run.
 *
 * Every level holds one `file-<n>.txt`, so a `file-*` search has a match arriving
 * every level rather than one burst at the end — which is what lets a spec watch a
 * result list (or a handed-off pane) GROW.
 */
export function createWalkGround(): string {
  const root = walkGroundPath()
  fs.rmSync(root, { recursive: true, force: true })
  let dir = root
  for (let level = 0; level < WALK_GROUND_DEPTH; level++) {
    dir = path.join(dir, 'd')
    fs.mkdirSync(dir, { recursive: true })
    fs.writeFileSync(path.join(dir, `file-${String(level)}.txt`), 'walk ground\n')
  }
  return root
}

/** Removes the chain. Safe to call when it was never built. */
export function removeWalkGround(): void {
  fs.rmSync(walkGroundPath(), { recursive: true, force: true })
}

/**
 * Turns indexing off for the local drive and deletes its index, then proves both
 * took.
 *
 * `cmdr://indexing` lists every volume with a registered index, so a listing with no
 * `root` in it is the whole precondition: no instance, no database, nothing for a
 * search to read. Throws rather than returning a flag — a spec that runs on a
 * covered drive doesn't fail, it passes vacuously.
 */
export async function makeLocalVolumeUnindexed(): Promise<void> {
  await mcpCall('indexing', { action: 'disable', volumeId: LOCAL_VOLUME_ID })
  await mcpCall('indexing', { action: 'forget', volumeId: LOCAL_VOLUME_ID })
  const registered = await mcpReadResource('cmdr://indexing')
  if (registered.includes(`${LOCAL_VOLUME_ID} (`)) {
    throw new Error(
      `The local drive still has an index after disable + forget, so a search would read it instead of walking:\n${registered}`,
    )
  }
}

/**
 * Puts the drive back the way the instance launched: indexing on, a full scan done.
 *
 * Every later spec in the shard shares this app, and several of them (directory
 * sizes, the MCP indexing surface, the index-served search specs) need a fresh
 * index. Waiting for `fresh` rather than just asking for the scan is what keeps the
 * next spec off a half-built one.
 */
export async function restoreLocalVolumeIndex(): Promise<void> {
  await mcpCall('indexing', { action: 'enable', volumeId: LOCAL_VOLUME_ID })
  await mcpCall('await', {
    condition: 'index_status',
    volumeId: LOCAL_VOLUME_ID,
    value: 'fresh',
    timeoutSeconds: 30,
  })
  // `fresh` says the SCAN finished, which is not the same as "a search can answer
  // from it": the arena is a separate snapshot and the truncate-and-rebuild above
  // invalidated whatever one was loaded. A spec that opens the dialog into that
  // window gets an auto-applied run with nothing to serve (seen on the Linux lane,
  // where `search-recent` failed once and passed on the retry). Asking for a real
  // answer is what makes the handover complete.
  await expect
    .poll(async () => mcpCall('search', { pattern: 'file-a*', scope: `${getFixtureRoot()}/left`, limit: 1 }), {
      timeout: 20_000,
    })
    .toContain('file-a.txt')
}

/**
 * Makes sure the local index can answer for `scope`, repairing it if it can't.
 *
 * A search whose index holds nothing for its scope can PARK: when every frontier root
 * is already claimed by another walk, `search/execute/live_run.rs::wait_for_the_other_walk`
 * waits for that walk with no deadline (by design — the alternative is the empty answer
 * it exists to remove). A spec that treats the dialog's results as bounded then dies on
 * whatever wait it used, having proven nothing.
 *
 * Both preconditions are inherited, not caused: [`makeLocalVolumeUnindexed`] empties the
 * index and [`restoreLocalVolumeIndex`] puts it back from an `afterAll`, so a restore that
 * didn't complete lands on a later spec in the shard. An index-served run can't park, so
 * proving one is the whole guard.
 *
 * Cheap when it already holds: one MCP `search` and no repair, which is the normal case.
 * ❌ Don't replace this with a plain wait — the run parks forever, so no wait is long enough.
 */
export async function ensureLocalIndexAnswers(scope: string, pattern: string, expected: string): Promise<void> {
  const answers = async (): Promise<boolean> =>
    (await mcpCall('search', { pattern, scope, limit: 1 })).includes(expected)
  if (await answers()) return

  await restoreLocalVolumeIndex()
  await expect.poll(answers, { timeout: 20_000 }).toBe(true)
}
