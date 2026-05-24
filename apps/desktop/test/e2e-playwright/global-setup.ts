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

export default function globalSetup(): void {
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
