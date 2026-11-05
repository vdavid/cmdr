/**
 * Finds calls to JavaScript and DOM functions newer than Safari 15.4 in an emitted chunk.
 *
 * ## Why
 *
 * `build.target` lowers syntax, never runtime APIs, so a `toSorted()` ships as written and throws
 * `TypeError` on a Catalina Mac (Safari 15.6.1 at best). Unlike 0.59.0's regex it doesn't stop
 * startup: it breaks just the feature that calls it, which is exactly what a startup smoke test on
 * the Catalina Mac misses. The ES2022 `lib` pin (`src/webkit-floor.d.ts`) catches the app's own
 * code at type-check time; this scan catches dependencies and the DOM APIs TypeScript can't version.
 *
 * ## What it checks
 *
 * Calls (and `new`) only, matched by name: a static like `Object.groupBy(…)`, a method like
 * `.toSorted(…)` on any object, or a global like `requestIdleCallback(…)`. Merely reading a member
 * (`typeof Object.groupBy`) is fine. A call is fine when it sits in the branch of a check that names
 * the same function (`typeof x`, `'x' in window`, `x && x()`, `x ? x() : …`, `x?.()`), which is how
 * the bundle's existing uses detect the feature. A method name can collide with a library's own
 * method of that name; keep this list to names distinctive enough that none does.
 */

import { apiFloor, SAFARI_FLOOR } from './vite-webkit-floor-compat.ts'

/** A floor API a call names. */
interface FloorApi {
  /** `Object.groupBy`, `toSorted`, or `requestIdleCallback`. */
  label: string
  /** The bare name a feature check would mention: `groupBy`. */
  name: string
  since: string
}

const FLOOR = apiFloor(SAFARI_FLOOR)

/**
 * Uses a person checked by hand and found safe, each with the reason. `chunkPrefix` (the emitted
 * file name minus its hash) limits one to a single chunk. An entry here still can't hide the
 * app's OWN use of the name: the ES2022 `lib` pin fails it at type-check time.
 */
const REVIEWED: readonly { api: string; chunkPrefix?: string; why: string }[] = [
  {
    api: 'OffscreenCanvas',
    chunkPrefix: '_app/immutable/workers/measure-worker-',
    why: 'the font-measure worker starts only after `typeof OffscreenCanvas` passes on the main thread',
  },
  {
    api: 'Uint8Array.fromBase64',
    why: 'devalue (SvelteKit) picks its native base64 only when `typeof Uint8Array.fromBase64` is a function, through a variable this scan cannot follow',
  },
  { api: 'toBase64', why: 'devalue: the native encoder behind the same `fromBase64` check' },
  { api: 'with', why: "LogTape's own `Logger.with()`, not `Array.prototype.with`" },
  { api: 'drop', why: "LogTape's sink queue's own `drop()`, not the iterator helper" },
]

export interface ApiFinding {
  /** `Object.groupBy`, `toSorted`, or `requestIdleCallback`. */
  api: string
  since: string
}

interface AstNode {
  type?: string
  [key: string]: unknown
}

/** Calls `fn` on each child node (array items and object fields), skipping position fields. */
function eachChild(value: unknown, fn: (child: unknown) => void): void {
  if (Array.isArray(value)) {
    for (const item of value) fn(item)
    return
  }
  if (value === null || typeof value !== 'object') return
  const node = value as AstNode
  for (const key in node) {
    if (key !== 'type' && key !== 'start' && key !== 'end') fn(node[key])
  }
}

/** The name one node contributes to a feature check: an identifier, a member name, or a string. */
function nameOf(n: AstNode): string[] {
  if (n.type === 'Identifier' && typeof n.name === 'string') return [n.name]
  if (n.type === 'Literal' && typeof n.value === 'string') return [n.value]
  if (n.type !== 'TemplateLiteral') return []
  return (n.quasis as { value: { cooked: string | null } }[]).flatMap((q) => (q.value.cooked ? [q.value.cooked] : []))
}

/** Every name a feature check's test mentions: identifiers, member names, and strings. */
function namesIn(value: unknown, into: Set<string>): Set<string> {
  if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
    for (const name of nameOf(value as AstNode)) into.add(name)
  }
  eachChild(value, (child) => namesIn(child, into))
  return into
}

/** The floor API a call's callee names, if any. */
function calledApi(callee: AstNode): FloorApi | undefined {
  if (callee.type === 'Identifier' && typeof callee.name === 'string') {
    const since = FLOOR.globals.get(callee.name)
    return since === undefined ? undefined : { label: callee.name, name: callee.name, since }
  }
  if (callee.type !== 'MemberExpression' || callee.computed === true) return undefined
  const name = (callee.property as AstNode).name
  if (typeof name !== 'string') return undefined
  const object = callee.object as AstNode
  if (object.type === 'Identifier' && typeof object.name === 'string') {
    const label = `${object.name}.${name}`
    const since = FLOOR.statics.get(label)
    if (since !== undefined) return { label, name, since }
  }
  const since = FLOOR.methods.get(name)
  return since === undefined ? undefined : { label: name, name, since }
}

/** Every unchecked call to an API newer than Safari 15.4 in one parsed chunk. */
export function findApisBeyondSafari15(program: unknown, chunkFileName: string): ApiFinding[] {
  const exempt = new Set(
    REVIEWED.filter((e) => e.chunkPrefix === undefined || chunkFileName.startsWith(e.chunkPrefix)).map((e) => e.api),
  )
  // The minifier names its locals freely, so a `var ML=` must not read as the `ML` global.
  const declared = declaredNames(program)
  const findings: ApiFinding[] = []
  const checkCall = (node: AstNode, guards: ReadonlySet<string>) => {
    if (node.type !== 'CallExpression' && node.type !== 'NewExpression') return
    if (node.optional === true) return
    const callee = node.callee as AstNode
    if (callee.type === 'Identifier' && declared.has(callee.name as string)) return
    const api = calledApi(callee)
    if (api && !guards.has(api.name) && !exempt.has(api.label)) {
      findings.push({ api: api.label, since: api.since })
    }
  }
  const visit = (value: unknown, guards: ReadonlySet<string>): void => {
    if (value === null || typeof value !== 'object' || Array.isArray(value)) {
      eachChild(value, (child) => {
        visit(child, guards)
      })
      return
    }
    const node = value as AstNode
    const branch = guardedBranch(node)
    if (branch) {
      visit(branch.test, guards)
      visit(branch.guarded, namesIn(branch.test, new Set(guards)))
      visit(branch.unguarded, guards)
      return
    }
    checkCall(node, guards)
    eachChild(node, (child) => {
      visit(child, guards)
    })
  }
  visit(program, new Set())
  return findings
}

/** The binding-pattern fields that introduce a name. */
const DECLARING_FIELDS: Record<string, readonly string[]> = {
  VariableDeclarator: ['id'],
  FunctionDeclaration: ['id', 'params'],
  FunctionExpression: ['id', 'params'],
  ArrowFunctionExpression: ['params'],
  ClassDeclaration: ['id'],
  ClassExpression: ['id'],
  CatchClause: ['param'],
  ImportSpecifier: ['local'],
  ImportDefaultSpecifier: ['local'],
  ImportNamespaceSpecifier: ['local'],
}

/** Every name the chunk declares anywhere, scope-blind: enough to tell a local from a global. */
function declaredNames(program: unknown): Set<string> {
  const names = new Set<string>()
  const visit = (value: unknown): void => {
    if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
      const node = value as AstNode
      for (const field of DECLARING_FIELDS[node.type ?? ''] ?? []) namesIn(node[field], names)
    }
    eachChild(value, visit)
  }
  visit(program)
  return names
}

/** A feature check's parts: its test, the branch it protects, and the branch it doesn't. */
function guardedBranch(node: AstNode): { test: unknown; guarded: unknown; unguarded: unknown } | undefined {
  if (node.type === 'ConditionalExpression' || node.type === 'IfStatement') {
    return { test: node.test, guarded: node.consequent, unguarded: node.alternate }
  }
  if (node.type === 'LogicalExpression' && node.operator === '&&') {
    return { test: node.left, guarded: node.right, unguarded: undefined }
  }
  return undefined
}

/** The build error's lines for the API findings, grouped by chunk. */
export function describeApiFindings(byChunk: ReadonlyMap<string, readonly ApiFinding[]>): string[] {
  const lines = [
    'The bundle calls functions newer than Safari 15.4, the floor Cmdr promises (a fully patched macOS Catalina gets',
    'Safari 15.6.1). Nothing lowers a runtime API, so each call throws `TypeError` on that Mac. Use an older equivalent,',
    'or put the call behind a check that names the function (`typeof x === "function" ? x() : fallback`). If the',
    'chunk is a dependency, find who pulls it in. Why this check exists: `scripts/vite-webkit-api-floor.ts`.',
  ]
  for (const [chunk, findings] of byChunk) {
    for (const f of findings) lines.push(`  ${chunk}: ${f.api} (${f.since})`)
  }
  return lines
}
