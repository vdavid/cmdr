/**
 * Shared E2E fixture helper for creating deterministic test directories.
 *
 * Both macOS and Linux wdio configs import this to set up the fixture tree
 * that CMDR_E2E_START_PATH points to. Runs in the wdio Node.js process,
 * not in the browser.
 *
 * On macOS, when an instance ID is provided, bulk `.dat` files are hardlinked
 * from a shared content cache at `/tmp/cmdr-e2e-fixtures-cache/` instead of
 * being regenerated per shard. The cache is built once via the "tmp dir +
 * content-hash verify + atomic rename" protocol so two concurrent E2E runs
 * coexist with zero races. Text files are always full copies because tests
 * mutate them.
 *
 * Linux Docker E2E (single shard) stays on the legacy `/tmp/cmdr-e2e-<ts>/`
 * shared root with no cache; the cost of regenerating 170 MB once per run is
 * lower than the bookkeeping for one shard.
 */

import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'
import { execSync } from 'child_process'

const smallFileContent = 'A'.repeat(1024) // ~1 KB

// Tiny binary media fixtures (a few-KB PNG + a 1-page PDF) committed under
// `media-fixtures/`, copied into `left/` so the viewer media E2E specs have a
// real image and PDF to open. Kept as committed files (not generated) so the
// bytes are deterministic and reviewable.
const mediaFixturesDir = path.join(path.dirname(fileURLToPath(import.meta.url)), 'media-fixtures')
const mediaFixtures = [
  { rel: 'left/sample.png', source: 'sample.png' },
  { rel: 'left/sample.pdf', source: 'sample.pdf' },
] as const

function copyMediaFixtures(rootPath: string): void {
  for (const file of mediaFixtures) {
    const dest = path.join(rootPath, file.rel)
    fs.mkdirSync(path.dirname(dest), { recursive: true })
    fs.copyFileSync(path.join(mediaFixturesDir, file.source), dest)
  }
}

const fixtureLayout = {
  textFiles: [
    { rel: 'left/file-a.txt', content: smallFileContent },
    { rel: 'left/file-b.txt', content: smallFileContent },
    { rel: 'left/sub-dir/nested-file.txt', content: smallFileContent },
    { rel: 'left/.hidden-file', content: smallFileContent },
  ],
  directories: ['left/bulk', 'right'],
  largeFiles: [
    { rel: 'left/bulk/large-1.dat', sizeMb: 50 },
    { rel: 'left/bulk/large-2.dat', sizeMb: 50 },
    { rel: 'left/bulk/large-3.dat', sizeMb: 50 },
  ],
  mediumFiles: Array.from({ length: 20 }, (_, i) => ({
    rel: `left/bulk/medium-${String(i + 1).padStart(2, '0')}.dat`,
    sizeMb: 1,
  })),
} as const

const CACHE_ROOT = '/tmp/cmdr-e2e-fixtures-cache'
const CACHE_TMP_PREFIX = '/tmp/cmdr-e2e-fixtures-cache-tmp-'
const FIXTURE_PREFIX = '/tmp/cmdr-e2e-fixtures-'
const LEGACY_PREFIX = '/tmp/cmdr-e2e-'

function generateDatFile(filePath: string, sizeMb: number): void {
  execSync(`dd if=/dev/zero bs=1048576 count=${String(sizeMb)} of="${filePath}" 2>/dev/null`)
}

/**
 * Removes a single fixture entry, including the dangling-symlink edge case.
 *
 * `fs.rmSync(p, { recursive: true, force: true })` silently no-ops on a dangling
 * symlink (target missing), because `force: true` swallows the underlying
 * ENOENT. Iterating siblings can produce exactly that state: removing
 * `link-target.txt` BEFORE a sibling `my-link` symlink that points to it leaves
 * `my-link` dangling, then `rmSync` on `my-link` does nothing. We `lstat`
 * first and call `unlinkSync` directly on symlinks so they always get removed.
 */
function removeFixtureEntry(entry: string): void {
  let stat: fs.Stats | undefined
  try {
    stat = fs.lstatSync(entry)
  } catch {
    return
  }
  if (stat.isSymbolicLink()) {
    fs.unlinkSync(entry)
    return
  }
  fs.rmSync(entry, { recursive: true, force: true })
}

/**
 * Returns the path to the shared content cache.
 *
 * The cache holds zero-filled `.dat` files of the sizes listed in `fixtureLayout`.
 * Per-instance fixture roots hardlink from here instead of regenerating ~170 MB
 * per shard.
 */
export function getCacheRoot(): string {
  return CACHE_ROOT
}

function bulkFiles(): { rel: string; sizeMb: number }[] {
  return [...fixtureLayout.largeFiles, ...fixtureLayout.mediumFiles]
}

/**
 * Builds the cache if absent. Idempotent and concurrency-safe.
 *
 * Two parallel callers each find the cache missing, each builds into its own
 * `cache-tmp-<rand>/`, then `renameSync`s onto `CACHE_ROOT`. Whichever rename
 * wins is authoritative; the loser sees `ENOTEMPTY` on its rename, removes
 * its tmp dir, and proceeds to hardlink from the winner.
 *
 * On `EXDEV` (cross-filesystem rename, possible on Linux bind-mounts) the cache
 * directory is built in-place via copy and a warning is logged.
 */
export function ensureCacheBuilt(): void {
  if (cacheIsValid(CACHE_ROOT)) return

  const tmpDir = fs.mkdtempSync(CACHE_TMP_PREFIX)
  try {
    fs.mkdirSync(path.join(tmpDir, 'left/bulk'), { recursive: true })
    for (const file of bulkFiles()) {
      generateDatFile(path.join(tmpDir, file.rel), file.sizeMb)
    }
    if (!cacheIsValid(tmpDir)) {
      throw new Error(`Cache build verification failed in ${tmpDir}`)
    }
    try {
      fs.renameSync(tmpDir, CACHE_ROOT)
    } catch (err: unknown) {
      const code = (err as NodeJS.ErrnoException).code
      if (code === 'ENOTEMPTY' || code === 'EEXIST' || code === 'EISDIR') {
        // Another concurrent builder won. Their cache is authoritative.
        fs.rmSync(tmpDir, { recursive: true, force: true })
      } else if (code === 'EXDEV') {
        console.warn(`Cache rename hit EXDEV; falling back to copy into ${CACHE_ROOT}`)
        copyDirRecursive(tmpDir, CACHE_ROOT)
        fs.rmSync(tmpDir, { recursive: true, force: true })
      } else {
        throw err
      }
    }
  } catch (err) {
    fs.rmSync(tmpDir, { recursive: true, force: true })
    throw err
  }
}

function copyDirRecursive(src: string, dest: string): void {
  fs.mkdirSync(dest, { recursive: true })
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const srcPath = path.join(src, entry.name)
    const destPath = path.join(dest, entry.name)
    if (entry.isDirectory()) {
      copyDirRecursive(srcPath, destPath)
    } else {
      fs.copyFileSync(srcPath, destPath)
    }
  }
}

/**
 * Returns true iff every bulk file in the cache exists and has the expected size.
 *
 * Size check is sufficient because content is deterministic zero-fill (no content
 * variation possible). A torn write from a crashed mid-build process would leave
 * a short file, which a size check catches. We avoid a sha256 check on 170 MB at
 * every E2E start since the verify cost would dominate the cache benefit; the
 * atomic-rename protocol prevents readers from ever seeing a partial cache.
 */
function cacheIsValid(root: string): boolean {
  for (const file of bulkFiles()) {
    const filePath = path.join(root, file.rel)
    let stat: fs.Stats
    try {
      stat = fs.statSync(filePath)
    } catch {
      return false
    }
    const expectedSize = file.sizeMb * 1024 * 1024
    if (stat.size !== expectedSize) return false
  }
  return true
}

/**
 * Hardlinks every bulk file from the cache into `targetRoot`.
 *
 * Falls back to `copyFileSync` on `EXDEV` (cross-filesystem; should not happen
 * for two paths under `/tmp` but we keep the safety net).
 */
function hardlinkBulkFromCache(targetRoot: string): void {
  for (const file of bulkFiles()) {
    const cacheFile = path.join(CACHE_ROOT, file.rel)
    const fixtureFile = path.join(targetRoot, file.rel)
    fs.mkdirSync(path.dirname(fixtureFile), { recursive: true })
    try {
      fs.linkSync(cacheFile, fixtureFile)
    } catch (err: unknown) {
      const code = (err as NodeJS.ErrnoException).code
      if (code === 'EXDEV') {
        console.warn(`Hardlink hit EXDEV for ${fixtureFile}; falling back to copy`)
        fs.copyFileSync(cacheFile, fixtureFile)
      } else {
        throw err
      }
    }
  }
}

/**
 * Creates the fixture tree.
 *
 * When `instanceId` is set (macOS Playwright), the fixtures land in a per-instance
 * directory at `/tmp/cmdr-e2e-fixtures-<instance>-<ts>/`. Bulk `.dat` files are
 * hardlinked from the shared cache at `/tmp/cmdr-e2e-fixtures-cache/`; the cache
 * is built on first use. Text files are full copies because tests mutate them.
 *
 * When `instanceId` is unset (Linux Docker), legacy behavior: full `dd` regen
 * into `/tmp/cmdr-e2e-<ts>/`.
 */
export function createFixtures(instanceId?: string): string {
  const timestamp = Date.now()
  const rootPath = instanceId
    ? `${FIXTURE_PREFIX}${instanceId}-${String(timestamp)}`
    : `${LEGACY_PREFIX}${String(timestamp)}`

  for (const dir of fixtureLayout.directories) {
    fs.mkdirSync(path.join(rootPath, dir), { recursive: true })
  }

  for (const file of fixtureLayout.textFiles) {
    const filePath = path.join(rootPath, file.rel)
    fs.mkdirSync(path.dirname(filePath), { recursive: true })
    fs.writeFileSync(filePath, file.content)
  }

  copyMediaFixtures(rootPath)

  if (instanceId) {
    ensureCacheBuilt()
    hardlinkBulkFromCache(rootPath)
  } else {
    for (const file of fixtureLayout.largeFiles) {
      generateDatFile(path.join(rootPath, file.rel), file.sizeMb)
    }
    for (const file of fixtureLayout.mediumFiles) {
      generateDatFile(path.join(rootPath, file.rel), file.sizeMb)
    }
  }

  console.log(`Fixtures created at ${rootPath} (~170 MB${instanceId ? ', bulk hardlinked from cache' : ''})`)
  return rootPath
}

export function cleanupFixtures(rootPath: string): void {
  if (!rootPath.startsWith(FIXTURE_PREFIX) && !rootPath.startsWith(LEGACY_PREFIX)) {
    throw new Error(`Refusing to delete path outside ${LEGACY_PREFIX}*: ${rootPath}`)
  }
  fs.rmSync(rootPath, { recursive: true, force: true })
  console.log(`Fixtures cleaned up: ${rootPath}`)
}

/**
 * Lightweight per-test fixture recreation.
 *
 * Only recreates the small text files and directory structure, not the ~170 MB
 * bulk .dat files. Those are created once in `createFixtures()` (called from
 * `onPrepare`) and persist across tests.
 *
 * This avoids a multi-second window where the watched directories disappear and
 * get rebuilt, which could crash the Tauri app or kill the WebDriver session.
 * Instead, we surgically remove and recreate only the small files that tests
 * might have moved/deleted.
 */
export function recreateFixtures(rootPath: string): void {
  if (!rootPath.startsWith(FIXTURE_PREFIX) && !rootPath.startsWith(LEGACY_PREFIX)) {
    throw new Error(`Refusing to recreate path outside ${LEGACY_PREFIX}*: ${rootPath}`)
  }

  // Clean up left/ text files and sub-dir (tests may have moved/deleted them),
  // but preserve left/bulk/ which has the large .dat files from onPrepare.
  const leftDir = path.join(rootPath, 'left')
  if (fs.existsSync(leftDir)) {
    for (const entry of fs.readdirSync(leftDir)) {
      if (entry === 'bulk') continue // preserve bulk .dat files
      removeFixtureEntry(path.join(leftDir, entry))
    }
  }

  // Clean up right/ contents (tests may have copied/moved files into it).
  // Preserve the directory itself to keep the app's inotify watch intact;
  // deleting and recreating would invalidate the watch (new inode).
  const rightDir = path.join(rootPath, 'right')
  if (fs.existsSync(rightDir)) {
    for (const entry of fs.readdirSync(rightDir)) {
      removeFixtureEntry(path.join(rightDir, entry))
    }
  }

  // Recreate directories (left/ and right/ already exist, ensure bulk/ exists)
  for (const dir of fixtureLayout.directories) {
    fs.mkdirSync(path.join(rootPath, dir), { recursive: true })
  }

  // Recreate text files
  for (const file of fixtureLayout.textFiles) {
    const filePath = path.join(rootPath, file.rel)
    fs.mkdirSync(path.dirname(filePath), { recursive: true })
    fs.writeFileSync(filePath, file.content)
  }

  // Re-copy the media fixtures: they live under left/, which the cleanup above wiped.
  copyMediaFixtures(rootPath)

  // Bulk .dat files are NOT recreated; they persist from createFixtures()
}

// Allow running directly for testing: npx tsx apps/desktop/test/e2e-shared/fixtures.ts
if (process.argv[1]?.endsWith('fixtures.ts')) {
  try {
    const instanceId = process.env.CMDR_INSTANCE_ID
    const root = createFixtures(instanceId)
    console.log('Self-test passed. Cleaning up...')
    cleanupFixtures(root)
    console.log('Done.')
  } catch (err: unknown) {
    console.error('Self-test failed:', err)
    process.exit(1)
  }
}
