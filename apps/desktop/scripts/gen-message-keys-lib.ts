/**
 * Pure logic for the message-key codegen, factored out of the CLI
 * (`gen-message-keys.ts`) so it's unit-testable against in-memory inputs.
 *
 * Responsibilities:
 *  - parse the `messages/en/*.json` catalogs into the sorted `MessageKey` set
 *    (dropping ARB-style `@key` metadata entries),
 *  - scan frontend source for `t()`/`tString()`/`getMessage()`/`<Trans key=…>`
 *    key usages,
 *  - diff the two to report keys used-but-missing (a build failure) and
 *    catalog-keys-never-used (a dead-key warning),
 *  - emit the `keys.gen.ts` source.
 *
 * Key shape is a dotted path (`area.feature.leaf`); the codegen does not
 * validate the shape (the `message-key-naming` Go check owns that), it only
 * reflects what's in the catalog and what the code references.
 */

/**
 * Drops ARB-style `@key` metadata entries from one parsed catalog file, keeping
 * only the renderable message keys, in file order.
 */
export function stripMetadataKeys(raw: Record<string, unknown>): string[] {
  return Object.keys(raw).filter((key) => !key.startsWith('@'))
}

/**
 * Merges every message key across all parsed catalog files into one sorted,
 * deduped list.
 * @param files filename → parsed JSON
 */
export function collectCatalogKeys(files: Record<string, Record<string, unknown>>): string[] {
  const keys = new Set<string>()
  for (const raw of Object.values(files)) {
    for (const key of stripMetadataKeys(raw)) keys.add(key)
  }
  return [...keys].sort()
}

/**
 * Matches a message-key reference in source. Covers the runtime accessors
 * (`t`, `tString`, `getMessage`) called with a single-quoted/double-quoted/
 * backtick-no-expression string literal, and the `<Trans key="…">` /
 * `<Trans key='…'>` markup attribute. Dynamic (non-literal) keys are not
 * matchable and are intentionally skipped (see the dead-key honesty caveat in
 * the docs: a dynamically-built key reads as "unused" to this scan).
 */
const KEY_REFERENCE_RE =
  // t('x') | tString("x") | getMessage(`x`)
  /\b(?:t|tString|getMessage)\(\s*(['"`])([a-zA-Z0-9_.]+)\1/g
const TRANS_KEY_RE = /<Trans\b[^>]*?\bkey=(['"])([a-zA-Z0-9_.]+)\1/g
// `labelKey: 'x'` / `descriptionKey: "x"`: a key STORED in a `*Key`/`*Keys`
// property and resolved later through `t(variable)` (the settings registry
// pattern). The key is "used" at its literal definition site even though the
// `t()` call takes a variable. Conservative: only matches a dotted lowerCamel
// path value, so it can't catch arbitrary strings.
const KEY_PROPERTY_RE = /\b[a-zA-Z]*[Kk]eys?\s*:\s*(['"])([a-z][a-zA-Z0-9]*(?:\.[a-zA-Z0-9]+)+)\1/g

/**
 * Extracts every statically-resolvable message key referenced in a source
 * string. A key with an interpolated/template-expression form can't be read
 * statically and is skipped on purpose.
 */
export function extractUsedKeys(source: string): Set<string> {
  const used = new Set<string>()
  for (const re of [KEY_REFERENCE_RE, TRANS_KEY_RE, KEY_PROPERTY_RE]) {
    re.lastIndex = 0
    let match: RegExpExecArray | null
    while ((match = re.exec(source)) !== null) {
      used.add(match[2])
    }
  }
  return used
}

/**
 * Of the given catalog keys, returns those whose exact text appears anywhere in
 * `source`. Used ONLY for dead-key suppression: a key reached through a Record
 * map or other indirection (the section-title / tint-color maps, the registry's
 * `labelKey` fields) still has its literal in source, so a substring hit marks
 * it "not dead" even when `extractUsedKeys` (strict, for missing detection) saw
 * no direct reference. Scoped to the catalog (a substring scan over real keys),
 * so it never produces a phantom "missing key"; it only narrows `dead`.
 */
export function findCatalogKeyMentions(source: string, catalogKeys: string[]): Set<string> {
  const mentioned = new Set<string>()
  for (const key of catalogKeys) {
    if (source.includes(key)) mentioned.add(key)
  }
  return mentioned
}

/**
 * Arguments to `diffKeys`.
 *
 * - `usedKeys`: keys from direct reference forms (drives missing detection).
 * - `literalKeys` (optional): keys whose exact literal appears anywhere in
 *   source; suppresses the dead warning for keys reached through indirection.
 */
export interface DiffKeysArgs {
  catalogKeys: string[]
  usedKeys: Set<string>
  literalKeys?: Set<string>
}

/** The result of `diffKeys`. */
export interface KeyDiff {
  /** Used in code, absent from the catalog (a build failure). */
  missing: string[]
  /** In the catalog, never referenced in code (a warning). */
  dead: string[]
}

/**
 * Diffs the catalog keys against the keys referenced in code.
 */
export function diffKeys({ catalogKeys, usedKeys, literalKeys }: DiffKeysArgs): KeyDiff {
  const catalogSet = new Set(catalogKeys)
  const missing = [...usedKeys].filter((key) => !catalogSet.has(key)).sort()
  const dead = catalogKeys.filter((key) => !usedKeys.has(key) && !(literalKeys?.has(key) ?? false)).sort()
  return { missing, dead }
}

/**
 * Renders the `keys.gen.ts` source for a sorted key list.
 */
export function emitKeysModule(keys: string[]): string {
  const union = keys.length > 0 ? keys.map((k) => `  | '${k}'`).join('\n') : '  never'
  return `// AUTO-GENERATED by scripts/gen-message-keys.ts. Do not edit by hand.
// Run \`pnpm intl:keys\` (or the check pipeline) to refresh.

/** Every key present in \`messages/en/*.json\`. A wrong key is a typecheck error. */
export type MessageKey =
${union}
`
}
