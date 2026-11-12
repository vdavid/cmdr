/**
 * ESLint rule: a platform test never chooses copy.
 *
 * Per-OS wording lives in ONE catalog value, a whole-value
 * `{platform, select, macos {…} windows {…} other {…}}` block that the runtime
 * resolves (`src/lib/intl/DETAILS.md` § Platform variants). A conditional on the
 * OS whose arms resolve copy is the old shape: a `.mac` / `.other` key pair and a
 * ternary picking one. It hides the Windows wording behind Linux's (`isMacOS()`
 * has no Windows answer), splits one sentence into two keys translators can't see
 * side by side, and keeps the OS decision out of the catalog the checks read.
 * Whether a surface EXISTS is a capability question (`hasCapability()`), and that
 * stays legal, copy and all.
 *
 * Flags a ternary, an `if`, a Svelte `{#if}`, an `&&` / `||`, a `switch`, or a
 * lookup (`{ macos: … }[getPlatform()]`) whose TEST calls `isMacOS()` or
 * `getPlatform()` (directly, or through a name bound to one: `const mac =
 * isMacOS()`) and whose ARMS resolve copy: a call to `t`, `tString`, `tStringIn`,
 * or `getMessage`, a `<Trans>` element, or a string literal shaped like a message
 * key (`commands.fileGetInfo.label`, `fileLocked.suggestion`: dotted
 * camelCase segments). An OS test choosing a key glyph (`'⇧F8'` vs `'Shift+F8'`), a
 * modifier name, or behavior passes.
 *
 * Opt out per line with a reason when a platform test truly must pick copy:
 *
 *   // eslint-disable-next-line cmdr/no-platform-copy-branch -- <reason>
 */

/** The `#lib/platform` readers that answer "which OS?". */
const PLATFORM_FUNCTIONS = new Set(['isMacOS', 'getPlatform'])

/** The runtime's copy resolvers (`#lib/intl`). */
const COPY_FUNCTIONS = new Set(['t', 'tString', 'tStringIn', 'getMessage'])

/** A string literal shaped like a message key: two or more dotted camelCase segments. */
const MESSAGE_KEY_SHAPE = /^[a-z][A-Za-z0-9]*(?:\.[A-Za-z0-9]+)+$/

/** AST keys that point back up or sideways, never into a node's own subtree. */
const SKIPPED_KEYS = new Set(['parent', 'loc', 'range', 'tokens', 'comments'])

/** Calls `visit` on `node` and every node below it, until `visit` returns true. */
function someNode(node, visit) {
  if (!node || typeof node.type !== 'string') return false
  if (visit(node)) return true
  for (const [key, value] of Object.entries(node)) {
    if (SKIPPED_KEYS.has(key)) continue
    if (Array.isArray(value)) {
      if (value.some((child) => someNode(child, visit))) return true
    } else if (value && typeof value === 'object' && typeof value.type === 'string') {
      if (someNode(value, visit)) return true
    }
  }
  return false
}

/** The plain name a call's callee goes by: `isMacOS()`, `platform.isMacOS()`. */
function calleeName(call) {
  const { callee } = call
  if (callee.type === 'Identifier') return callee.name
  if (callee.type === 'MemberExpression' && !callee.computed && callee.property.type === 'Identifier') {
    return callee.property.name
  }
  return undefined
}

const isPlatformCall = (node) => node.type === 'CallExpression' && PLATFORM_FUNCTIONS.has(calleeName(node))

/** Whether a subtree resolves copy: a resolver call, a `<Trans>`, or a key-shaped string literal. */
function resolvesCopy(node) {
  return someNode(node, (child) => {
    if (child.type === 'CallExpression') return COPY_FUNCTIONS.has(calleeName(child))
    if (child.type === 'Literal') return typeof child.value === 'string' && MESSAGE_KEY_SHAPE.test(child.value)
    if (child.type === 'SvelteElement') return child.name?.type === 'Identifier' && child.name.name === 'Trans'
    return false
  })
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: 'Forbid an OS test (`isMacOS()`, `getPlatform()`) that chooses copy; use a platform block.',
      recommended: true,
    },
    messages: {
      platformCopyBranch:
        'An OS test picks copy here. Write one key whose value is a `{platform, select, macos {…} windows {…} other {…}}` ' +
        'block and let the runtime pick; gate a surface that only exists on some OSes with `hasCapability()`. ' +
        'See `src/lib/intl/DETAILS.md` § Platform variants.',
    },
    schema: [],
  },
  create(context) {
    // Names bound to a platform test (`const mac = isMacOS()`), by name: the rule runs per file, and
    // a declaration is visited before the conditionals that read it.
    const platformNames = new Set()
    const testsPlatform = (test) =>
      someNode(test, (node) => isPlatformCall(node) || (node.type === 'Identifier' && platformNames.has(node.name)))
    const check = (node, test, arms) => {
      if (testsPlatform(test) && arms.some((arm) => resolvesCopy(arm))) {
        context.report({ node, messageId: 'platformCopyBranch' })
      }
    }
    return {
      VariableDeclarator(node) {
        if (node.id.type === 'Identifier' && node.init && someNode(node.init, isPlatformCall)) {
          platformNames.add(node.id.name)
        }
      },
      ConditionalExpression(node) {
        check(node, node.test, [node.consequent, node.alternate])
      },
      IfStatement(node) {
        check(node, node.test, [node.consequent, node.alternate])
      },
      LogicalExpression(node) {
        // `isMacOS() && tString(…)`: the left side decides whether the right side's copy shows.
        check(node, node.left, [node.right])
      },
      SwitchStatement(node) {
        check(node, node.discriminant, node.cases)
      },
      MemberExpression(node) {
        // `{ macos: 'a.b', windows: 'c.d' }[getPlatform()]`: the OS indexes a table of copy.
        if (node.computed) check(node, node.property, [node.object])
      },
      SvelteIfBlock(node) {
        // `{:else if}` is a nested SvelteIfBlock whose own test is checked on its own visit.
        check(node, node.expression, [...node.children, node.else])
      },
    }
  },
}
