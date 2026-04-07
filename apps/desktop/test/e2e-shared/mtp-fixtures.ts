/**
 * MTP fixture helper for E2E tests.
 *
 * Recreates the virtual MTP device's backing directory tree so that each test
 * run starts from a known state. Runs in the Node.js process, not in the
 * browser/webview.
 */

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
 * Recreates the MTP fixture directory tree from scratch.
 *
 * Deletes ALL contents of `internal/` and `readonly/` (tests like F7 mkdir
 * create artifacts), preserves the root directories themselves (to keep the
 * virtual device's backing dir inodes stable), then recreates the fixture
 * file tree.
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

  // eslint-disable-next-line no-console
  console.log(`MTP fixtures created at ${root}`)
}

/**
 * Removes the entire MTP fixture tree. Only accepts paths under /tmp/cmdr-mtp-.
 */
export function cleanupMtpFixtures(): void {
  if (!MTP_FIXTURE_ROOT.startsWith('/tmp/cmdr-mtp-')) {
    throw new Error(`Refusing to delete path outside /tmp/cmdr-mtp-*: ${MTP_FIXTURE_ROOT}`)
  }
  fs.rmSync(MTP_FIXTURE_ROOT, { recursive: true, force: true })
  // eslint-disable-next-line no-console
  console.log(`MTP fixtures cleaned up: ${MTP_FIXTURE_ROOT}`)
}

// Allow running directly for testing: npx tsx apps/desktop/test/e2e-shared/mtp-fixtures.ts
if (process.argv[1]?.endsWith('mtp-fixtures.ts')) {
  try {
    recreateMtpFixtures()
    // eslint-disable-next-line no-console
    console.log('Self-test passed. Cleaning up...')
    cleanupMtpFixtures()
    // eslint-disable-next-line no-console
    console.log('Done.')
  } catch (err: unknown) {
    // eslint-disable-next-line no-console
    console.error('Self-test failed:', err)
    process.exit(1)
  }
}
