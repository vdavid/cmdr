/**
 * Builds the analytics settings-defaults manifest: the map the analytics dashboard needs to turn
 * "this key is absent from the heartbeat config" into a real answer.
 *
 * Why it exists. `settings.json` is sparse on purpose (only keys an actor explicitly set are
 * persisted, see `src/lib/settings/CLAUDE.md`), and the heartbeat ships that sparse object verbatim
 * as its config shape. So the wire carries deviation, never adoption: an absent `indexing.enabled`
 * means either "still on the default" or "this setting didn't exist in that build", and the row
 * itself can't tell those apart. Sending every default with every beat would answer it and was
 * rejected as wasteful, so the answer is resolved on the dashboard instead, joined on the
 * `app_version` every heartbeat already carries.
 *
 * What's in a snapshot. Exactly the keys that CAN appear in the config shape, mirroring
 * `src-tauri/src/analytics/config_shape.rs::include_key`: every registry setting whose default is a
 * boolean or a number, plus the string-valued ones named in that file's `CATEGORICAL_STRING_KEYS`.
 * Keys that reach the config shape without being settings (`fdaGranted`, `_schemaVersion`) are
 * absent by construction: they're not in the registry, they have no default, and they're always
 * present anyway, so resolution never asks about them.
 *
 * The registry is parsed, not imported. `definitions/appearance.ts` pulls in `$lib/intl`, whose
 * `messages.svelte.ts` is rune-compiled, so a bare `node` import of the registry can't evaluate.
 * Parsing the definition objects out of the TypeScript AST gets the same data with no Svelte
 * toolchain, and it works against a `git show` of an old tag, which is what makes the historical
 * backfill possible.
 */

import ts from 'typescript'
import { execFileSync } from 'node:child_process'
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

/** One setting id mapped to the value a fresh install runs with. */
export type DefaultsSnapshot = Record<string, boolean | number | string>

/**
 * The manifest as committed. `versions` is sparse along the version axis but COMPLETE within each
 * entry: every entry lists every key that existed in that release, so "absent from the resolved
 * entry" always means "the setting did not exist yet". A delta encoding couldn't say that.
 */
export interface DefaultsManifest {
  /** Written into the file so a human opening it knows what it is and what regenerates it. */
  note: string[]
  /** Released versions whose defaults differ from the entry before them, oldest key first. */
  versions: Record<string, DefaultsSnapshot>
  /** The working tree's snapshot. Never resolved against (it hasn't shipped); promoted at release. */
  next: DefaultsSnapshot
}

/** A registry default the parser couldn't evaluate, on a setting that CAN reach the config shape. */
export interface UnresolvedDefault {
  id: string
  file: string
  /** The source text of the `default:` initializer, for the failure message. */
  expression: string
}

export interface SnapshotResult {
  snapshot: DefaultsSnapshot
  /** Non-empty means the snapshot has a blind spot; the caller decides whether that's fatal. */
  unresolved: UnresolvedDefault[]
  /**
   * False for revisions whose heartbeat sent no config shape at all. Those releases have nothing to
   * resolve, so the backfill skips them rather than writing an entry that implies we know something.
   */
  configShapeShipped: boolean
}

const DEFINITIONS_DIR = 'apps/desktop/src/lib/settings/definitions'
const REGISTRY_FILE = 'apps/desktop/src/lib/settings/settings-registry.ts'
const CONFIG_SHAPE_FILE = 'apps/desktop/src-tauri/src/analytics/config_shape.rs'
/** Where the allowlist lived before `config_shape.rs` was split out of the analytics module. */
const CONFIG_SHAPE_FALLBACK_FILE = 'apps/desktop/src-tauri/src/analytics/mod.rs'

/** Setting types whose persisted value is a boolean or a number, so it always reaches the shape. */
const NUMERIC_OR_BOOLEAN_TYPES = new Set(['boolean', 'number', 'duration'])

/**
 * A read-only view of one revision of the repo. The working tree and any git ref both satisfy it,
 * which is the whole trick behind backfilling the manifest from release tags.
 */
export interface SourceTree {
  /** Files directly under `dir` (names only), or `[]` when the directory isn't in this revision. */
  list(dir: string): string[]
  /** File contents, or `null` when the file isn't in this revision. */
  read(path: string): string | null
}

/** The checkout on disk. */
export function workingTree(rootDir: string): SourceTree {
  return {
    list(dir) {
      try {
        return readdirSync(join(rootDir, dir)).sort()
      } catch {
        return []
      }
    },
    read(path) {
      try {
        return readFileSync(join(rootDir, path), 'utf8')
      } catch {
        return null
      }
    },
  }
}

/** A git ref (a release tag), read without touching the working tree. */
export function gitTree(rootDir: string, ref: string): SourceTree {
  const git = (args: string[]): string | null => {
    try {
      return execFileSync('git', args, { cwd: rootDir, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] })
    } catch {
      return null
    }
  }
  return {
    list(dir) {
      const out = git(['ls-tree', '--name-only', `${ref}:${dir}`])
      return out === null ? [] : out.split('\n').filter(Boolean).sort()
    },
    read(path) {
      return git(['show', `${ref}:${path}`])
    },
  }
}

/**
 * The registry source files in this revision. The definitions were split out of
 * `settings-registry.ts` into `definitions/*.ts` partway through the project's life, so both
 * layouts have to work for the backfill to reach the older releases.
 */
function registryFiles(tree: SourceTree): string[] {
  const definitions = tree.list(DEFINITIONS_DIR).filter((name) => name.endsWith('.ts') && !name.includes('.test.'))
  if (definitions.length > 0) return definitions.map((name) => `${DEFINITIONS_DIR}/${name}`)
  return [REGISTRY_FILE]
}

/** One parsed registry entry: enough to decide inclusion and to record the default. */
interface RegistryEntry {
  id: string
  /** The `type` field, which says whether this setting can reach the config shape at all. */
  type: string | null
  value: boolean | number | string | null
  /** Set when `value` is null because the initializer wasn't a literal we evaluate. */
  expression: string | null
  file: string
}

/**
 * Evaluates a `default:` initializer. Only literal shapes resolve, plus the one build-time constant
 * the registry uses: `import.meta.env.DEV` inlines to a boolean at build time, and the manifest
 * describes SHIPPED builds, so it resolves to `false`.
 *
 * Arrays and objects deliberately return `null` with no expression: they can never reach the config
 * shape (`include_key` drops them), so they're skipped rather than reported as a blind spot.
 */
function evaluateDefault(node: ts.Expression): { value: boolean | number | string | null; unresolved: boolean } {
  if (ts.isStringLiteral(node)) return { value: node.text, unresolved: false }
  if (ts.isNumericLiteral(node)) return { value: Number(node.text), unresolved: false }
  if (node.kind === ts.SyntaxKind.TrueKeyword) return { value: true, unresolved: false }
  if (node.kind === ts.SyntaxKind.FalseKeyword) return { value: false, unresolved: false }
  if (
    ts.isPrefixUnaryExpression(node) &&
    node.operator === ts.SyntaxKind.MinusToken &&
    ts.isNumericLiteral(node.operand)
  ) {
    return { value: -Number(node.operand.text), unresolved: false }
  }
  if (ts.isArrayLiteralExpression(node) || ts.isObjectLiteralExpression(node)) {
    return { value: null, unresolved: false }
  }
  if (isImportMetaEnvDev(node)) return { value: false, unresolved: false }
  return { value: null, unresolved: true }
}

/** Matches `import.meta.env.DEV` exactly, so a different meta property still counts as unresolved. */
function isImportMetaEnvDev(node: ts.Expression): boolean {
  if (!ts.isPropertyAccessExpression(node) || node.name.text !== 'DEV') return false
  const env = node.expression
  if (!ts.isPropertyAccessExpression(env) || env.name.text !== 'env') return false
  return ts.isMetaProperty(env.expression) && env.expression.keywordToken === ts.SyntaxKind.ImportKeyword
}

/** Reads a property's initializer off an object literal, when it's a plain `name: value` pair. */
function property(object: ts.ObjectLiteralExpression, name: string): ts.Expression | null {
  for (const member of object.properties) {
    if (!ts.isPropertyAssignment(member)) continue
    const key = member.name
    const keyText = ts.isIdentifier(key) || ts.isStringLiteral(key) ? key.text : null
    if (keyText === name) return member.initializer
  }
  return null
}

/**
 * Collects every registry definition in one file: any object literal carrying both an `id` and a
 * `default`. Nothing else in these files has that pair (enum options carry `value`/`label`,
 * constraints carry neither), so the shape is a reliable fingerprint without resolving imports.
 */
function parseFile(source: string, file: string): RegistryEntry[] {
  const parsed = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true)
  const entries: RegistryEntry[] = []

  const visit = (node: ts.Node): void => {
    if (ts.isObjectLiteralExpression(node)) {
      const idNode = property(node, 'id')
      const defaultNode = property(node, 'default')
      if (idNode !== null && defaultNode !== null && ts.isStringLiteral(idNode)) {
        const typeNode = property(node, 'type')
        const evaluated = evaluateDefault(defaultNode)
        entries.push({
          id: idNode.text,
          type: typeNode !== null && ts.isStringLiteral(typeNode) ? typeNode.text : null,
          value: evaluated.value,
          expression: evaluated.unresolved ? defaultNode.getText(parsed) : null,
          file,
        })
      }
    }
    ts.forEachChild(node, visit)
  }
  visit(parsed)
  return entries
}

/**
 * The `CATEGORICAL_STRING_KEYS` allowlist from `config_shape.rs`. It's read out of the Rust rather
 * than duplicated here so the manifest's key set can never claim a string key the backend drops,
 * and so a backfilled version gets the allowlist THAT release shipped with.
 */
export function parseCategoricalKeys(tree: SourceTree): Set<string> | null {
  const source = tree.read(CONFIG_SHAPE_FILE) ?? tree.read(CONFIG_SHAPE_FALLBACK_FILE)
  if (source === null) return null
  const block = /CATEGORICAL_STRING_KEYS\s*:\s*&\[&str\]\s*=\s*&\[([\s\S]*?)\];/.exec(source)
  if (block === null) return null
  return new Set([...block[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]))
}

/**
 * The defaults snapshot for one revision: every setting that can reach the config shape, at the
 * value a fresh install runs with.
 */
export function buildSnapshot(tree: SourceTree): SnapshotResult {
  const categorical = parseCategoricalKeys(tree)
  if (categorical === null) return { snapshot: {}, unresolved: [], configShapeShipped: false }
  const snapshot: DefaultsSnapshot = {}
  const unresolved: UnresolvedDefault[] = []

  for (const file of registryFiles(tree)) {
    const source = tree.read(file)
    if (source === null) continue
    for (const entry of parseFile(source, file)) {
      if (entry.value === null) {
        // A default we couldn't evaluate only matters when the setting's type says it would reach
        // the config shape. Anything else (a `string-array`, a non-categorical string) is invisible
        // to analytics either way, so it's silently skipped.
        const reachesShape = entry.type !== null && (NUMERIC_OR_BOOLEAN_TYPES.has(entry.type) || entry.type === 'enum')
        if (entry.expression !== null && reachesShape) {
          unresolved.push({ id: entry.id, file: entry.file, expression: entry.expression })
        }
        continue
      }
      const included = typeof entry.value === 'string' ? categorical.has(entry.id) : true
      if (included) snapshot[entry.id] = entry.value
    }
  }

  return { snapshot: sortKeys(snapshot), unresolved, configShapeShipped: true }
}

/** Stable key order, so a regeneration only ever diffs where a default actually moved. */
export function sortKeys(snapshot: DefaultsSnapshot): DefaultsSnapshot {
  const sorted: DefaultsSnapshot = {}
  for (const key of Object.keys(snapshot).sort()) sorted[key] = snapshot[key]
  return sorted
}

/** Oldest first. Plain three-part numeric compare: every Cmdr version is `MAJOR.MINOR.PATCH`. */
export function compareVersionsAsc(a: string, b: string): number {
  const pa = a.split('.').map(Number)
  const pb = b.split('.').map(Number)
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const diff = (pa[i] ?? 0) - (pb[i] ?? 0)
    if (diff !== 0) return diff
  }
  return 0
}

export function snapshotsEqual(a: DefaultsSnapshot, b: DefaultsSnapshot): boolean {
  return JSON.stringify(sortKeys(a)) === JSON.stringify(sortKeys(b))
}

/**
 * Adds `version` to the manifest, keeping it sparse: an entry is written only when the release
 * actually changed a default (or added/removed a key) relative to the newest entry at or below it.
 * Re-promoting the same version is idempotent.
 */
export function promote(manifest: DefaultsManifest, version: string, snapshot: DefaultsSnapshot): DefaultsManifest {
  const previous = resolveEntry(manifest.versions, version, { excludeSelf: true })
  const versions = { ...manifest.versions }
  if (previous !== null && snapshotsEqual(previous, snapshot)) {
    delete versions[version]
  } else {
    versions[version] = sortKeys(snapshot)
  }
  return { ...manifest, versions: sortVersions(versions), next: sortKeys(snapshot) }
}

export function sortVersions(versions: Record<string, DefaultsSnapshot>): Record<string, DefaultsSnapshot> {
  const sorted: Record<string, DefaultsSnapshot> = {}
  for (const version of Object.keys(versions).sort(compareVersionsAsc)) sorted[version] = versions[version]
  return sorted
}

/**
 * The snapshot that applies to `version`: the newest entry at or below it, or `null` when the
 * manifest starts after that version (which means we can't say anything about that install).
 */
export function resolveEntry(
  versions: Record<string, DefaultsSnapshot>,
  version: string,
  options: { excludeSelf?: boolean } = {},
): DefaultsSnapshot | null {
  let best: string | null = null
  for (const candidate of Object.keys(versions)) {
    const order = compareVersionsAsc(candidate, version)
    if (order > 0 || (order === 0 && options.excludeSelf === true)) continue
    if (best === null || compareVersionsAsc(candidate, best) > 0) best = candidate
  }
  return best === null ? null : versions[best]
}

/** The committed file's text: pretty-printed JSON with a trailing newline, so diffs stay readable. */
export function serializeManifest(manifest: DefaultsManifest): string {
  return `${JSON.stringify(manifest, null, 2)}\n`
}
