/**
 * Playwright global setup: creates the shared fixture directory tree.
 *
 * Runs once before all tests. Sets `CMDR_E2E_START_PATH` so the app
 * and test helpers know where to find the fixture files.
 *
 * If CMDR_E2E_START_PATH is already set (app started externally with
 * its own fixture dir), we recreate the text files in that existing
 * dir to ensure a clean state but don't create a new directory.
 */

import fs from 'fs'
import { createFixtures, recreateFixtures } from '../e2e-shared/fixtures.js'
import { recreateMtpFixtures } from '../e2e-shared/mtp-fixtures.js'
import { clearAppDeathMarker } from './app-death.js'
import { WAIT_SCALE } from './wait-budget.js'

export default function globalSetup(): void {
  // Say it out loud when the budgets aren't the literals. Without this line a run stretched
  // to 4x looks identical in the log to one that wasn't, and "why did this take 20 minutes"
  // has no answer in the output. Silent at 1x, which is every hand run.
  if (WAIT_SCALE !== 1) console.log(`[e2e] wait budgets scaled ${String(WAIT_SCALE)}x (CMDR_E2E_WAIT_SCALE)`)

  // Ahead of the shard-kind gate below, because a marker left by a previous run outlives
  // the process that wrote it and would fail every test of this one before it got to touch
  // the app. Every shard kind starts from "the app is alive". See `app-death.ts`.
  clearAppDeathMarker()

  // ❗ The marketing captures are the shards with NO fixture tree: the stills photograph
  // the developer's REAL folders, and the clips stage a demo tree of their own
  // (`marketing-clips-demo.ts`). `CMDR_E2E_START_PATH` is what arms the post-test guard
  // that deletes anything not in the fixture manifest. Creating a tree here would also
  // SET that variable, so "the orchestrator leaves it unset" is only half the
  // protection; this is the other half. It skips `recreateMtpFixtures()` for the
  // ordinary reason too: the backing dir is shared, and a parallel MTP shard is real.
  const shardKind = process.env.CMDR_E2E_SHARD_KIND
  if (shardKind === 'marketing-shots' || shardKind === 'marketing-clips') return

  const existingRoot = process.env.CMDR_E2E_START_PATH
  if (existingRoot && fs.existsSync(existingRoot)) {
    // App already running with this fixture dir: refresh text files
    recreateFixtures(existingRoot)
  } else {
    // Pass the instance ID through to createFixtures so per-shard runs land
    // under /tmp/cmdr-e2e-fixtures-<instance>-<ts>/ and share the hardlink
    // cache. Linux Docker has no instance ID and stays on the legacy shared
    // root.
    const instanceId = process.env.CMDR_INSTANCE_ID
    const fixtureRoot = createFixtures(instanceId)
    process.env.CMDR_E2E_START_PATH = fixtureRoot
    globalThis.__PLAYWRIGHT_CREATED_FIXTURES = true
  }

  // Ensure clean MTP virtual device state (independent of local fixtures).
  // Under parallel sharding the MTP-backing dir is shared across all Tauri
  // instances, so only the dedicated MTP shard (or a non-sharded run) is
  // allowed to recreate it. Other shards skip this step.
  const skipMtp = process.env.CMDR_E2E_SKIP_MTP_FIXTURES === '1'
  if (!skipMtp) {
    recreateMtpFixtures()
  }
}
