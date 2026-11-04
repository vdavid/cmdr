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

/** A function newer than Safari 15.4, with the Safari that added it. */
interface FloorApi {
  /** `Object` for `Object.groupBy`; absent for a method (any receiver) or a global. */
  owner?: string
  name: string
  kind: 'static' | 'method' | 'global'
  since: string
}

const API_LIST: readonly FloorApi[] = [
  { owner: 'Object', name: 'groupBy', kind: 'static', since: 'Safari 17.4' },
  { owner: 'Map', name: 'groupBy', kind: 'static', since: 'Safari 17.4' },
  { owner: 'Promise', name: 'withResolvers', kind: 'static', since: 'Safari 17.4' },
  { owner: 'Promise', name: 'try', kind: 'static', since: 'Safari 18.2' },
  { owner: 'Array', name: 'fromAsync', kind: 'static', since: 'Safari 16.4' },
  { owner: 'Iterator', name: 'from', kind: 'static', since: 'Safari 18.4' },
  { owner: 'RegExp', name: 'escape', kind: 'static', since: 'Safari 18.4' },
  { owner: 'AbortSignal', name: 'timeout', kind: 'static', since: 'Safari 16.0' },
  { owner: 'AbortSignal', name: 'any', kind: 'static', since: 'Safari 17.4' },
  { owner: 'URL', name: 'canParse', kind: 'static', since: 'Safari 17.0' },
  { owner: 'URL', name: 'parse', kind: 'static', since: 'Safari 18.0' },
  { owner: 'Atomics', name: 'waitAsync', kind: 'static', since: 'Safari 16.4' },
  { owner: 'Intl', name: 'DurationFormat', kind: 'static', since: 'Safari 16.4' },
  { name: 'toSorted', kind: 'method', since: 'Safari 16.0' },
  { name: 'toReversed', kind: 'method', since: 'Safari 16.0' },
  { name: 'toSpliced', kind: 'method', since: 'Safari 16.0' },
  { name: 'isWellFormed', kind: 'method', since: 'Safari 16.4' },
  { name: 'toWellFormed', kind: 'method', since: 'Safari 16.4' },
  { name: 'union', kind: 'method', since: 'Safari 17.0 (Set)' },
  { name: 'intersection', kind: 'method', since: 'Safari 17.0 (Set)' },
  { name: 'symmetricDifference', kind: 'method', since: 'Safari 17.0 (Set)' },
  { name: 'isSubsetOf', kind: 'method', since: 'Safari 17.0 (Set)' },
  { name: 'isSupersetOf', kind: 'method', since: 'Safari 17.0 (Set)' },
  { name: 'isDisjointFrom', kind: 'method', since: 'Safari 17.0 (Set)' },
  { name: 'checkVisibility', kind: 'method', since: 'Safari 17.4' },
  { name: 'showPopover', kind: 'method', since: 'Safari 17.0' },
  { name: 'togglePopover', kind: 'method', since: 'Safari 17.0' },
  { name: 'requestIdleCallback', kind: 'global', since: 'not in Safari 15' },
  { name: 'OffscreenCanvas', kind: 'global', since: 'Safari 16.4' },
  { name: 'CompressionStream', kind: 'global', since: 'Safari 16.4' },
  { name: 'DecompressionStream', kind: 'global', since: 'Safari 16.4' },
]

/**
 * A chunk that may use an API unchecked, because nothing loads it without a check elsewhere.
 * Matched on the emitted file name minus its hash, so it survives rebuilds.
 */
const EXEMPT_CHUNKS: readonly { prefix: string; api: string; why: string }[] = [
  {
    prefix: '_app/immutable/workers/measure-worker-',
    api: 'OffscreenCanvas',
    why: 'the font-measure worker starts only after `typeof OffscreenCanvas` passes on the main thread',
  },
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

const label = (api: FloorApi) => (api.owner ? `${api.owner}.${api.name}` : api.name)

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
  if (callee.type === 'Identifier') {
    return API_LIST.find((api) => api.kind === 'global' && api.name === callee.name)
  }
  if (callee.type !== 'MemberExpression' || callee.computed === true) return undefined
  const property = (callee.property as AstNode).name
  const object = callee.object as AstNode
  return API_LIST.find(
    (api) =>
      api.name === property &&
      (api.kind === 'method' || (api.kind === 'static' && object.type === 'Identifier' && object.name === api.owner)),
  )
}

/** Every unchecked call to an API newer than Safari 15.4 in one parsed chunk. */
export function findApisBeyondSafari15(program: unknown, chunkFileName: string): ApiFinding[] {
  const exempt = new Set(EXEMPT_CHUNKS.filter((e) => chunkFileName.startsWith(e.prefix)).map((e) => e.api))
  const findings: ApiFinding[] = []
  const checkCall = (node: AstNode, guards: ReadonlySet<string>) => {
    if (node.type !== 'CallExpression' && node.type !== 'NewExpression') return
    if (node.optional === true) return
    const api = calledApi(node.callee as AstNode)
    if (api && !guards.has(api.name) && !exempt.has(label(api))) {
      findings.push({ api: label(api), since: api.since })
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
