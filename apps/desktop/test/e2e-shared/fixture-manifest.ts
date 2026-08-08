/**
 * Shared-fixture drift guard: does `left/` + `right/` still look pristine?
 *
 * The E2E fixture tree is shared by every spec on a shard, and a spec that
 * mutates it without restoring hands the next spec a tree it can't recognize.
 * The failure then lands inside the VICTIM's `ensureAppReady` ("expected files
 * not found"), with membership that shifts by shard order. `diffFixtureTree`
 * compares the tree on disk against the layout `fixtures.ts` declares, so the
 * global `afterEach` guard can fail the spec that actually dirtied it.
 *
 * Two properties matter and are pinned by `fixture-manifest.test.ts`:
 *
 * - **It reads the filesystem only.** No pane, no watcher, no flush, so it
 *   can't trip over how a displayed listing gets replaced.
 * - **`restoreFixtureTree` touches only what drifted.** A blunt
 *   delete-and-rewrite gives every file a new inode, which cuts the archive
 *   specs' watch of `sample.zip` out from under them (measured 0/4 on
 *   `archive-browsing`, `docs/notes/flake-corpus-2026-08-08.md`).
 *
 * Scope: `left/` and `right/`, the two directories `recreateFixtures` owns and
 * `ensureAppReady` asserts on. Spec-owned fixture dirs beside them
 * (`brief-cursor-fixtures/`, `full-cursor-page-nav-fixtures/`) are deliberately
 * long-lived and are not the guard's business.
 */

import fs from 'fs'
import path from 'path'
import crypto from 'crypto'
import { pristineFixtureEntries, removeFixtureEntry, restoreBulkFile, type FixtureSpecEntry } from './fixtures.js'

/** The directories the guard owns. Everything else in the root is a spec's own. */
const GUARDED_DIRS = ['left', 'right']

/**
 * Content check by expected size: files this small get hashed, so a
 * same-length overwrite (the conflict specs write 1 KB of different text) and
 * an in-place archive edit are both caught. Bigger files are checked by size
 * only, which covers every mutation a spec actually makes to the read-only
 * bulk `.dat` tree while keeping the post-test cost at ~20 `lstat` calls.
 */
const HASH_SIZE_LIMIT = 512 * 1024

interface ExpectedEntry {
  kind: 'dir' | 'file'
  size?: number
  hash?: string
  restore: FixtureSpecEntry
}

interface ActualEntry {
  kind: 'dir' | 'file' | 'symlink'
  size: number
}

export interface FixtureTreeDiff {
  /** Paths present on disk that the pristine tree doesn't have. */
  added: string[]
  /** Paths the pristine tree has that are gone from disk. */
  removed: string[]
  /** Paths whose kind, size, or content no longer matches. */
  changed: string[]
}

function hashFile(absPath: string): string {
  return crypto.createHash('sha1').update(fs.readFileSync(absPath)).digest('hex')
}

/**
 * The pristine manifest, derived from the fixture layout and the committed
 * media / archive sources. Computed once per process: the sources are a handful
 * of KB and they can't change while a suite runs.
 */
let cachedExpected: Map<string, ExpectedEntry> | null = null

function expectedManifest(): Map<string, ExpectedEntry> {
  if (cachedExpected) return cachedExpected

  const expected = new Map<string, ExpectedEntry>()
  for (const entry of pristineFixtureEntries()) {
    if (!GUARDED_DIRS.some((dir) => entry.rel === dir || entry.rel.startsWith(`${dir}/`))) continue

    if (entry.kind === 'dir') {
      expected.set(entry.rel, { kind: 'dir', restore: entry })
      continue
    }
    if (entry.source.type === 'text') {
      const content = Buffer.from(entry.source.content)
      expected.set(entry.rel, {
        kind: 'file',
        size: content.byteLength,
        hash: crypto.createHash('sha1').update(content).digest('hex'),
        restore: entry,
      })
      continue
    }
    if (entry.source.type === 'copy') {
      const size = fs.statSync(entry.source.from).size
      expected.set(entry.rel, {
        kind: 'file',
        size,
        hash: size <= HASH_SIZE_LIMIT ? hashFile(entry.source.from) : undefined,
        restore: entry,
      })
      continue
    }
    expected.set(entry.rel, { kind: 'file', size: entry.source.sizeMb * 1024 * 1024, restore: entry })
  }

  cachedExpected = expected
  return expected
}

/** Walks one guarded directory, recording every entry by its root-relative path. */
function walk(rootPath: string, rel: string, out: Map<string, ActualEntry>): void {
  let stat: fs.Stats
  try {
    stat = fs.lstatSync(path.join(rootPath, rel))
  } catch {
    return // Missing: the expected-side comparison reports it as removed.
  }

  if (stat.isSymbolicLink()) {
    out.set(rel, { kind: 'symlink', size: stat.size })
    return
  }
  if (!stat.isDirectory()) {
    out.set(rel, { kind: 'file', size: stat.size })
    return
  }

  out.set(rel, { kind: 'dir', size: 0 })
  for (const name of fs.readdirSync(path.join(rootPath, rel))) {
    walk(rootPath, `${rel}/${name}`, out)
  }
}

/**
 * Compares `left/` + `right/` under `rootPath` against the pristine layout.
 *
 * Returns `null` when the tree is pristine, so a caller can treat it as a
 * plain "is it clean?" check. Costs one `lstat` per entry (~60) plus a hash of
 * the ~10 small files; measured at ~1 ms on an M3 Max.
 */
export function diffFixtureTree(rootPath: string): FixtureTreeDiff | null {
  const expected = expectedManifest()
  const actual = new Map<string, ActualEntry>()
  for (const dir of GUARDED_DIRS) walk(rootPath, dir, actual)

  const added: string[] = []
  const removed: string[] = []
  const changed: string[] = []

  for (const [rel, want] of expected) {
    const got = actual.get(rel)
    if (!got) {
      removed.push(rel)
      continue
    }
    if (got.kind !== want.kind) {
      changed.push(rel)
      continue
    }
    if (want.kind === 'dir') continue
    if (got.size !== want.size) {
      changed.push(rel)
      continue
    }
    if (want.hash !== undefined && hashFile(path.join(rootPath, rel)) !== want.hash) {
      changed.push(rel)
    }
  }

  for (const rel of actual.keys()) {
    if (!expected.has(rel)) added.push(rel)
  }

  if (added.length === 0 && removed.length === 0 && changed.length === 0) return null
  return { added: added.sort(), removed: removed.sort(), changed: changed.sort() }
}

/** Renders a diff as one `<verb>: <path>` line per drifted entry. */
export function describeFixtureTreeDiff(diff: FixtureTreeDiff): string {
  return [
    ...diff.added.map((rel) => `  added: ${rel}`),
    ...diff.removed.map((rel) => `  removed: ${rel}`),
    ...diff.changed.map((rel) => `  changed: ${rel}`),
  ].join('\n')
}

function restoreEntry(rootPath: string, entry: FixtureSpecEntry): void {
  const target = path.join(rootPath, entry.rel)
  if (entry.kind === 'dir') {
    removeFixtureEntry(target)
    fs.mkdirSync(target, { recursive: true })
    return
  }
  fs.mkdirSync(path.dirname(target), { recursive: true })
  if (entry.source.type === 'text') {
    removeFixtureEntry(target)
    fs.writeFileSync(target, entry.source.content)
    return
  }
  if (entry.source.type === 'copy') {
    removeFixtureEntry(target)
    fs.copyFileSync(entry.source.from, target)
    return
  }
  restoreBulkFile(rootPath, entry.rel, entry.source.sizeMb)
}

/**
 * Restores `left/` + `right/` to pristine, touching only the entries that
 * drifted.
 *
 * A clean tree costs one `diffFixtureTree` and changes nothing at all: no
 * rewrite, no new inode, so a watch an archive spec holds on `sample.zip`
 * survives. Extras are removed deepest-first so a directory's children are
 * gone before the directory itself.
 */
export function restoreFixtureTree(rootPath: string): void {
  const diff = diffFixtureTree(rootPath)
  if (!diff) return

  for (const rel of [...diff.added].sort((a, b) => b.length - a.length)) {
    removeFixtureEntry(path.join(rootPath, rel))
  }

  const expected = expectedManifest()
  // Shallowest first, so a missing parent directory exists before its children.
  const restorable = [...diff.removed, ...diff.changed].sort((a, b) => a.length - b.length)
  for (const rel of restorable) {
    const want = expected.get(rel)
    if (want) restoreEntry(rootPath, want.restore)
  }
}
