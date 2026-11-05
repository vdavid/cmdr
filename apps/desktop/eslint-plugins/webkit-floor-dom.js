/**
 * ESLint rule (type-aware): flag a DOM call the Safari 15.4 floor doesn't have, unless a feature
 * check guards it.
 *
 * Why: a fully patched Catalina Mac runs Cmdr on Safari 15.6.1's WebKit, and `build.target` lowers
 * syntax, never runtime APIs. A `el.checkVisibility()` (Safari 17.4) ships as written and throws
 * `TypeError` there, breaking just the feature that calls it. TypeScript's DOM lib can't be pinned
 * to a Safari version the way `lib: ES2022` pins the JavaScript built-ins, and a by-name scan of
 * the bundle can't tell `el.format()` from a library's own `format()`. Here TypeScript says which
 * declaration a call resolves to, so the rule looks up exactly that entry in MDN's
 * browser-compat-data: `api.Element.checkVisibility`, `api.AbortSignal.timeout_static`, or a
 * window global like `api.Window.requestIdleCallback`.
 *
 * Passes: a call inside the branch of a check that names the member (`typeof x`, `'x' in el`,
 * `x && x()`, `x ? x() : …`, an `if`), and an optional call (`el.checkVisibility?.()`).
 * A member BCD files under a different interface than TypeScript's (a mixin) has no entry and
 * passes: a miss, never a false alarm. The bundle scan (`scripts/vite-webkit-api-floor.ts`)
 * covers dependencies; the reasoning behind both: `docs/notes/system-requirements-and-es2025.md`
 * § The two floors old WebKit crosses.
 *
 * Opt out per line, with the reason:
 *   // eslint-disable-next-line cmdr/webkit-floor-dom -- <why it's safe>
 */

import ts from 'typescript'
import { domMemberSince, SAFARI_FLOOR } from '../scripts/vite-webkit-floor-compat.ts'

/** The lib files whose declarations are the browser's DOM. */
const DOM_LIB = /[\\/]typescript[\\/]lib[\\/]lib\.(dom|webworker)[\w.]*\.d\.ts$/

/** `{ iface, member }` for a declaration in the DOM lib, in BCD's naming, or null. */
function bcdKeyOf(declaration, name) {
  if (!DOM_LIB.test(declaration.getSourceFile().fileName)) return null
  const parent = declaration.parent
  // `interface Element { checkVisibility(): boolean }`
  if (ts.isInterfaceDeclaration(parent)) return { iface: parent.name.text, member: name }
  // `declare var AbortSignal: { timeout(ms: number): AbortSignal }`: a static.
  if (ts.isTypeLiteralNode(parent) && ts.isVariableDeclaration(parent.parent)) {
    return { iface: parent.parent.name.getText(), member: `${name}_static` }
  }
  // `declare function requestIdleCallback(…)`: a global, which BCD files under Window.
  if (ts.isFunctionDeclaration(declaration)) return { iface: 'Window', member: name }
  // `declare var OffscreenCanvas: { new(…): OffscreenCanvas }`: a global constructor.
  if (ts.isVariableDeclaration(declaration)) return { iface: name, member: null }
  return null
}

/** Every name a feature check's test mentions: identifiers, member names, and strings. */
function namesIn(node, into = new Set()) {
  if (!node || typeof node !== 'object') return into
  if (node.type === 'Identifier') into.add(node.name)
  if (node.type === 'Literal' && typeof node.value === 'string') into.add(node.value)
  if (node.type === 'TemplateLiteral') for (const q of node.quasis) into.add(q.value.cooked ?? '')
  for (const key of Object.keys(node)) {
    if (key === 'parent' || key === 'loc' || key === 'range') continue
    const child = node[key]
    if (Array.isArray(child)) for (const c of child) namesIn(c, into)
    else if (child && typeof child.type === 'string') namesIn(child, into)
  }
  return into
}

/** True when a feature check that names `name` encloses `node` on its guarded side. */
function isGuarded(node, name) {
  for (let child = node, parent = node.parent; parent; child = parent, parent = parent.parent) {
    const guardedBranch =
      ((parent.type === 'ConditionalExpression' || parent.type === 'IfStatement') && parent.consequent === child) ||
      (parent.type === 'LogicalExpression' && parent.operator === '&&' && parent.right === child)
    const test = parent.type === 'LogicalExpression' ? parent.left : parent.test
    if (guardedBranch && namesIn(test).has(name)) return true
  }
  return false
}

export default {
  meta: {
    type: 'problem',
    docs: { description: 'Flag a DOM call newer than the Safari 15.4 floor without a feature check' },
    schema: [],
    messages: {
      tooNew:
        '`{{api}}` isn\'t in Safari 15.4 ({{since}}), the WebKit a fully patched Catalina Mac runs Cmdr on, so this call throws there. Use an older equivalent, or guard it: `typeof {{callee}} === "function" ? … : fallback`.',
    },
  },
  create(context) {
    const services = context.sourceCode.parserServices
    if (!services?.program) return {}
    const checker = services.program.getTypeChecker()

    function check(node, callee) {
      if (node.optional || callee.optional) return
      const target = callee.type === 'MemberExpression' && !callee.computed ? callee.property : callee
      if (target.type !== 'Identifier') return
      const symbol = checker.getSymbolAtLocation(services.esTreeNodeToTSNodeMap.get(target))
      const resolved = symbol && symbol.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(symbol) : symbol
      for (const declaration of resolved?.declarations ?? []) {
        const key = bcdKeyOf(declaration, target.name)
        if (!key) continue
        const since = domMemberSince(key.iface, key.member, SAFARI_FLOOR)
        if (since === undefined || isGuarded(node, target.name)) return
        const member = key.member?.replace(/_static$/, '')
        const api = member === undefined ? key.iface : key.iface === 'Window' ? member : `${key.iface}.${member}`
        context.report({ node, messageId: 'tooNew', data: { api, since, callee: context.sourceCode.getText(callee) } })
        return
      }
    }

    return {
      CallExpression(node) {
        check(node, node.callee)
      },
      NewExpression(node) {
        check(node, node.callee)
      },
    }
  },
}
