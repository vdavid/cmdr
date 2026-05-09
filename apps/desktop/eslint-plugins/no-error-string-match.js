/**
 * ESLint rule: ban substring-matching against error/state semantics.
 *
 * Rationale: classifying errors or app state by `<x>.message.includes('foo')`
 * couples behavior to user-facing wording that's free to change. The right tool
 * is a typed enum variant, an error code, or an explicit flag on the struct
 * crossing the IPC boundary. Mirrors the Rust-side `error-string-match` check
 * in `scripts/check/checks/desktop-rust-error-string-match.go`.
 *
 * Triggers on `<receiver>.{includes,startsWith,endsWith}('<literal>')` where
 * `<receiver>` is a member access (or a call chain that bottoms out in a member
 * access) whose property name is in the suspicious set: `message`, `error`,
 * `errorMessage`, `stderr`, `stdout`, `reason`, `title`, `userAgent`.
 *
 * Wrapping calls like `.toLowerCase()`, `.toString()`, and `.trim()` are peeled
 * off so `friendly.title.toLowerCase().includes('no permission')` still
 * resolves to the `title` member access.
 *
 * Opt out per-line with the standard ESLint comment when a substring match
 * really is the right tool (third-party SDK with no error code, etc.):
 *
 *   // eslint-disable-next-line custom/no-error-string-match -- <reason>
 */

const SUSPICIOUS_PROPERTIES = new Set([
  'message',
  'error',
  'errorMessage',
  'stderr',
  'stdout',
  'reason',
  'title',
  'userAgent',
])

const STRING_PEEL_METHODS = new Set(['toLowerCase', 'toUpperCase', 'toString', 'trim', 'trimStart', 'trimEnd'])

const MATCHER_METHODS = new Set(['includes', 'startsWith', 'endsWith'])

/**
 * Peel off wrapping string method calls (`.toLowerCase()`, `.trim()`, …) and
 * return the inner node, or null if the chain doesn't bottom out in a usable
 * receiver.
 */
function peelStringMethodCalls(node) {
  let current = node
  while (
    current?.type === 'CallExpression' &&
    current.callee.type === 'MemberExpression' &&
    !current.callee.computed &&
    current.callee.property.type === 'Identifier' &&
    STRING_PEEL_METHODS.has(current.callee.property.name)
  ) {
    current = current.callee.object
  }
  return current
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: 'Forbid substring-matching against error/state values; use typed flags instead.',
      recommended: true,
    },
    messages: {
      stringMatchOnError:
        "Substring-matching `{{ property }}` is fragile — it couples behavior to wording that's free to change. " +
        'Use a typed flag from the backend struct, an error code, or a discriminated-union variant. ' +
        'See AGENTS.md § "No string-matching error or state classification".',
    },
    schema: [],
  },
  create(context) {
    return {
      CallExpression(node) {
        const callee = node.callee
        if (
          callee.type !== 'MemberExpression' ||
          callee.computed ||
          callee.property.type !== 'Identifier' ||
          !MATCHER_METHODS.has(callee.property.name)
        ) {
          return
        }

        if (node.arguments.length < 1) return
        const firstArg = node.arguments[0]
        const isStringLiteral =
          (firstArg.type === 'Literal' && typeof firstArg.value === 'string') || firstArg.type === 'TemplateLiteral'
        if (!isStringLiteral) return

        const receiver = peelStringMethodCalls(callee.object)
        if (
          !receiver ||
          receiver.type !== 'MemberExpression' ||
          receiver.computed ||
          receiver.property.type !== 'Identifier'
        ) {
          return
        }

        const propName = receiver.property.name
        if (!SUSPICIOUS_PROPERTIES.has(propName)) return

        context.report({
          node,
          messageId: 'stringMatchOnError',
          data: { property: propName },
        })
      },
    }
  },
}
