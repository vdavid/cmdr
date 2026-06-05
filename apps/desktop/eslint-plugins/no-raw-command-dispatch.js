/**
 * ESLint rule: ban string literals as command ids in dispatch calls.
 *
 * Rationale (invariant A3): the command bus dispatches `CommandId`-typed values
 * end to end. A string literal like `handleCommandExecute('file.rename')` sneaks
 * a magic string past the type system — it compiles only because the literal
 * happens to be assignable to the union today, and silently rots the moment a
 * command is renamed. Every dispatch site must pass a `CommandId`-typed value
 * (a registry lookup, a narrowed payload, a forwarded prop), not a literal.
 *
 * This is the command-bus twin of `no-raw-tauri-invoke`: keep magic strings out
 * of cross-layer dispatch so renames are caught at compile time.
 *
 * ## What this rule catches
 *
 * A string-literal (or template-literal-with-no-expressions) FIRST argument to a
 * call whose callee name is a known dispatch entry point:
 *   - `dispatch('file.rename')`
 *   - `handleCommandExecute('file.rename', ctx)`
 *   - `onExecute('file.rename')` / `onCommand('file.rename')` (the prop channels)
 * Renamed-import aliases are intentionally NOT chased (same stance as
 * `no-raw-tauri-invoke`): an alias is extra effort that reads as suspicious in
 * review.
 *
 * ## What it deliberately does NOT catch
 *
 * - Calls that pass a non-literal (a variable, a member access, a narrowed
 *   value). Those are the typed path the rule wants you on.
 * - The registry file, test files, and `/test/` dirs (allowed-path fragments):
 *   the registry IS where command-id literals legitimately live, and tests poke
 *   the dispatcher with literal ids on purpose.
 *
 * Opt out per-line if you really must:
 *
 *   // eslint-disable-next-line cmdr/no-raw-command-dispatch -- <reason>
 */

// Path fragments that opt a file out entirely. The registry declares the id
// literals; tests dispatch literal ids on purpose.
const allowedPathFragments = ['/command-registry.ts', '/command-ids.ts', '.test.', '/test/']

// Call-site names that mean "dispatch a command". A literal first argument to any
// of these is a raw command-id string.
const DISPATCH_CALLEES = new Set(['dispatch', 'handleCommandExecute', 'dispatchCommand', 'onExecute', 'onCommand'])

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: 'Pass a `CommandId`-typed value to the command bus, never a raw string literal.',
      recommended: true,
    },
    messages: {
      rawDispatch:
        "Don't dispatch the string literal `'{{ command }}'`. Pass a `CommandId`-typed value so the id is " +
        'checked at compile time and survives a registry rename (invariant A3). Look it up from the registry, ' +
        'narrow it with `isCommandId`, or forward an already-typed value. See `lib/commands/CLAUDE.md`.',
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename || context.getFilename() || ''
    if (allowedPathFragments.some((fragment) => filename.includes(fragment))) {
      return {}
    }

    return {
      CallExpression(node) {
        const callee = node.callee
        // Match a bare identifier call (`dispatch(...)`) or a member call
        // (`ctx.dispatch(...)`, `explorer.onCommand(...)`) by the final name.
        let calleeName
        if (callee.type === 'Identifier') {
          calleeName = callee.name
        } else if (callee.type === 'MemberExpression' && !callee.computed && callee.property.type === 'Identifier') {
          calleeName = callee.property.name
        } else {
          return
        }
        if (!DISPATCH_CALLEES.has(calleeName)) return
        if (node.arguments.length < 1) return

        const firstArg = node.arguments[0]
        const literalString =
          firstArg.type === 'Literal' && typeof firstArg.value === 'string'
            ? firstArg.value
            : firstArg.type === 'TemplateLiteral' && firstArg.expressions.length === 0
              ? firstArg.quasis[0].value.cooked
              : undefined
        if (literalString === undefined) return

        context.report({
          node: firstArg,
          messageId: 'rawDispatch',
          data: { command: literalString },
        })
      },
    }
  },
}
