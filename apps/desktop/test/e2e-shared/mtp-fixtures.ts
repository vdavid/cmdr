/**
 * MTP fixture helper for E2E tests.
 *
 * Recreates the virtual MTP device's backing directory tree so that each test
 * run starts from a known state. Runs in the Node.js process, not in the
 * browser/webview.
 */

import { randomUUID } from 'crypto'
import fs from 'fs'
import path from 'path'

// Must match Rust constant at src-tauri/src/mtp/virtual_device.rs::MTP_FIXTURE_ROOT
export const MTP_FIXTURE_ROOT = '/tmp/cmdr-mtp-e2e-fixtures'

const fixtureLayout = {
  directories: ['internal/Documents', 'internal/DCIM', 'internal/DCIM/Burst', 'internal/Music', 'readonly/photos'],
  files: [
    {
      rel: 'internal/Documents/report.txt',
      content: 'Quarterly report \u2014 Q4 2025 placeholder content.\n',
    },
    {
      rel: 'internal/Documents/notes.txt',
      content: 'Meeting notes: discuss MTP E2E test strategy.\n',
    },
    {
      rel: 'internal/DCIM/photo-001.jpg',
      content: Buffer.from([0xff, 0xd8, 0xff, 0xe0, ...Buffer.from('dummy-jpeg-bytes')]),
    },
    {
      rel: 'internal/DCIM/Burst/burst-001.jpg',
      content: Buffer.from([0xff, 0xd8, 0xff, 0xe0, ...Buffer.from('dummy-burst-bytes')]),
    },
    {
      rel: 'readonly/photos/sunset.jpg',
      content: Buffer.from([0xff, 0xd8, 0xff, 0xe0, ...Buffer.from('dummy-sunset-bytes')]),
    },
  ],
} as const

/**
 * Filename prefix for drain-sentinel files (see `writeMtpDrainSentinel`). The
 * Rust IPC `resync_virtual_mtp_after_disk_change` polls the watcher for any
 * dropped path ending with the unique full sentinel name.
 */
export const MTP_SENTINEL_PREFIX = '.cmdr-drain-'

/**
 * Recreates the MTP fixture directory tree from scratch.
 *
 * Deletes ALL contents of `internal/` and `readonly/` (tests like F7 mkdir
 * create artifacts), preserves the root directories themselves (to keep the
 * virtual device's backing dir inodes stable), then recreates the fixture
 * file tree.
 *
 * **Pair with `writeMtpDrainSentinel()` as the very last step before calling
 * the resync IPC**, so the backend can wait for the sentinel's FS event to
 * land. Per-directory ordering on every supported `notify` backend means all
 * earlier writes will already have arrived by then — replacing the previous
 * fixed-duration FSEvents-latency sleeps with actual observed quiescence.
 */
export function recreateMtpFixtures(): void {
  const root = MTP_FIXTURE_ROOT

  // Ensure the top-level root and storage dirs exist
  const internalDir = path.join(root, 'internal')
  const readonlyDir = path.join(root, 'readonly')
  fs.mkdirSync(internalDir, { recursive: true })
  fs.mkdirSync(readonlyDir, { recursive: true })

  // Wipe all contents of internal/ and readonly/ (preserve the dirs themselves)
  for (const storageDir of [internalDir, readonlyDir]) {
    for (const entry of fs.readdirSync(storageDir)) {
      fs.rmSync(path.join(storageDir, entry), { recursive: true, force: true })
    }
  }

  // Create directories
  for (const dir of fixtureLayout.directories) {
    fs.mkdirSync(path.join(root, dir), { recursive: true })
  }

  // Create files
  for (const file of fixtureLayout.files) {
    const filePath = path.join(root, file.rel)
    fs.writeFileSync(filePath, file.content)
  }

  console.log(`MTP fixtures created at ${root}`)
}

/**
 * Writes a unique sentinel file inside the MTP backing dir's `internal/` and
 * returns its filename. Pass the returned name to
 * `resync_virtual_mtp_after_disk_change`'s `sentinelSuffix` parameter: the
 * backend polls the watcher's dropped-paths ring until it sees this exact
 * filename, then rescans and resumes. Per-directory FS-event ordering means
 * every write that happened before this sentinel has been observed by the
 * watcher by then — so no fixed FSEvents-latency sleep is needed.
 *
 * **Always call this AFTER all other fixture mutations** (recreate + any
 * additional seeding). Calling it earlier and then writing more files would
 * race: the backend would see the sentinel, declare the queue drained, and
 * resume the watcher while later events were still in flight.
 */
export function writeMtpDrainSentinel(): string {
  const sentinelName = `${MTP_SENTINEL_PREFIX}${randomUUID()}`
  fs.writeFileSync(path.join(MTP_FIXTURE_ROOT, 'internal', sentinelName), '')
  return sentinelName
}

/**
 * Removes the entire MTP fixture tree. Only accepts paths under /tmp/cmdr-mtp-.
 */
export function cleanupMtpFixtures(): void {
  if (!MTP_FIXTURE_ROOT.startsWith('/tmp/cmdr-mtp-')) {
    throw new Error(`Refusing to delete path outside /tmp/cmdr-mtp-*: ${MTP_FIXTURE_ROOT}`)
  }
  fs.rmSync(MTP_FIXTURE_ROOT, { recursive: true, force: true })
  console.log(`MTP fixtures cleaned up: ${MTP_FIXTURE_ROOT}`)
}

// Allow running directly for testing: npx tsx apps/desktop/test/e2e-shared/mtp-fixtures.ts
if (process.argv[1]?.endsWith('mtp-fixtures.ts')) {
  try {
    recreateMtpFixtures()
    console.log('Self-test passed. Cleaning up...')
    cleanupMtpFixtures()
    console.log('Done.')
  } catch (err: unknown) {
    console.error('Self-test failed:', err)
    process.exit(1)
  }
}
