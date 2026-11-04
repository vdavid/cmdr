/**
 * Finds regular expressions in an emitted chunk that the oldest WebKit Cmdr promises can't
 * compile. `webkitFloor` in `vite-webkit-floor.ts` runs it on every client chunk.
 *
 * ## Why the build target doesn't cover this
 *
 * `build.target: 'safari15'` lowers SYNTAX, but a regex feature has no lowering. When the source
 * writes a regex literal the target can't parse, the bundler rewrites it as `RegExp(`…`, `u`)`
 * and moves on without a warning. That trades a parse error for a runtime `SyntaxError` at the
 * line that builds it, and a module-level `const` builds it while the module evaluates. 0.59.0
 * shipped one lookbehind in `src/lib/utils/display-path.ts`, so the whole module graph failed to
 * evaluate on Catalina's Safari 15.6.1, before the logger existed: no frontend log line, and a
 * window the backend had to show 10 s later, blank.
 *
 * ## What it checks
 *
 * Every regex literal and every `RegExp(…)` / `new RegExp(…)` call whose pattern is a constant
 * string, in every emitted chunk, so dependency code counts too. A pattern built at runtime
 * from a variable can't be checked here. The features are the ones Safari 15.6 lacks: see
 * `BEYOND_SAFARI_15`. Evidence: `docs/notes/system-requirements-and-es2025.md` § The two floors old WebKit crosses (regex syntax).
 */

import { RegExpParser, visitRegExpAST } from '@eslint-community/regexpp'
import { parseSync } from 'vite'

/** Regex features Safari 15.6 (the newest WebKit a Catalina gets) can't compile, with the Safari that added each. */
export const BEYOND_SAFARI_15 = {
  lookbehind: 'a lookbehind assertion, `(?<=…)` or `(?<!…)` (Safari 16.4)',
  unicodeSets: 'the `v` flag (Safari 17.0)',
  duplicateNamedGroups: 'a capture group name used twice (Safari 17.0)',
  modifiers: 'an inline modifier group, `(?i:…)` (not in Safari 15)',
} as const

export type RegexFeature = keyof typeof BEYOND_SAFARI_15

export interface RegexFinding {
  /** The pattern as the bundle carries it, without slashes or flags. */
  pattern: string
  flags: string
  features: RegexFeature[]
}

const parser = new RegExpParser({ ecmaVersion: 2025 })

/** The Safari-15-incompatible features one pattern uses, empty when it's fine or unparsable. */
export function regexFeaturesBeyondSafari15(pattern: string, flags: string): RegexFeature[] {
  const found = new Set<RegexFeature>()
  if (flags.includes('v')) found.add('unicodeSets')
  let ast
  try {
    ast = parser.parsePattern(pattern, 0, pattern.length, {
      unicode: flags.includes('u'),
      unicodeSets: flags.includes('v'),
    })
  } catch {
    // An invalid pattern throws on every engine; that's a bug for tests to find, not a floor issue.
    return [...found]
  }
  const groupNames = new Set<string>()
  visitRegExpAST(ast, {
    onAssertionEnter(node) {
      if (node.kind === 'lookbehind') found.add('lookbehind')
    },
    onModifiersEnter() {
      found.add('modifiers')
    },
    onCapturingGroupEnter(node) {
      if (node.name === null) return
      if (groupNames.has(node.name)) found.add('duplicateNamedGroups')
      groupNames.add(node.name)
    },
  })
  return [...found]
}

interface AstNode {
  type?: string
  [key: string]: unknown
}

/** A string literal or an expression-free template literal's value, else `undefined`. */
function constantString(node: unknown): string | undefined {
  const n = node as AstNode | undefined
  if (n?.type === 'Literal' && typeof n.value === 'string') return n.value
  if (n?.type === 'TemplateLiteral' && (n.expressions as unknown[]).length === 0) {
    const quasis = n.quasis as { value: { cooked: string | null } }[]
    return quasis[0]?.value.cooked ?? undefined
  }
  return undefined
}

function isRegExpConstructor(node: AstNode): boolean {
  if (node.type !== 'CallExpression' && node.type !== 'NewExpression') return false
  const callee = node.callee as AstNode
  return callee.type === 'Identifier' && callee.name === 'RegExp'
}

/** Every constant regex in `code` (a whole emitted chunk) that Safari 15 can't compile. */
export function findRegexBeyondSafari15(code: string): RegexFinding[] {
  return findRegexInProgram(parseChunk(code))
}

/** Parses one emitted chunk, throwing rather than returning half an AST. */
export function parseChunk(code: string): unknown {
  const { program, errors } = parseSync('chunk.js', code, { sourceType: 'module' })
  // A chunk that doesn't parse is the bundler's bug, and checking half an AST would read as a pass.
  if (errors.length > 0) throw new Error(`couldn't parse an emitted chunk: ${errors[0]?.message ?? ''}`)
  return program
}

/** Every constant regex in a parsed chunk that Safari 15 can't compile. */
export function findRegexInProgram(program: unknown): RegexFinding[] {
  const findings: RegexFinding[] = []
  const check = (pattern: string, flags: string) => {
    const features = regexFeaturesBeyondSafari15(pattern, flags)
    if (features.length > 0) findings.push({ pattern, flags, features })
  }
  const visit = (value: unknown) => {
    if (Array.isArray(value)) {
      for (const item of value) visit(item)
      return
    }
    if (value === null || typeof value !== 'object') return
    const node = value as AstNode
    const regex = node.regex as { pattern: string; flags: string } | undefined
    if (node.type === 'Literal' && regex) check(regex.pattern, regex.flags)
    if (isRegExpConstructor(node)) {
      const args = node.arguments as unknown[]
      const pattern = constantString(args[0])
      const flags = args.length > 1 ? constantString(args[1]) : ''
      if (pattern !== undefined && flags !== undefined) check(pattern, flags)
    }
    for (const key in node) {
      if (key !== 'type' && key !== 'start' && key !== 'end') visit(node[key])
    }
  }
  visit(program)
  return findings
}

/** The build error's lines for the regex findings, grouped by chunk, with what to do about them. */
export function describeFindings(byChunk: ReadonlyMap<string, readonly RegexFinding[]>): string {
  const lines = [
    'The bundle holds regular expressions that Safari 15 (the newest WebKit a macOS Catalina Mac gets) can’t compile.',
    'The bundler can’t lower these: it rewrites each into a `RegExp()` call that throws at runtime, and one at module level',
    'stops the whole frontend from mounting. Rewrite each without the feature (for a lookbehind, match the preceding',
    'character and check it in a `replace` callback). Search the source for the pattern text.',
  ]
  for (const [chunk, findings] of byChunk) {
    for (const f of findings) {
      const why = f.features.map((feature) => BEYOND_SAFARI_15[feature]).join('; ')
      lines.push(`  ${chunk}: /${f.pattern}/${f.flags}: ${why}`)
    }
  }
  return lines.join('\n')
}
