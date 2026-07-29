/**
 * ESLint rule: ban hand-rolled key combos in keydown handlers.
 *
 * Rationale: a keydown handler must match a WHOLE combo. `e.key === 'a' && e.metaKey`
 * looks like "⌘A" but is a modifier SUPERSET — it is also true for ⌥⌘A, ⌃⌘A, ⇧⌘A. That
 * is literally how pressing ⌥⌘A (open Ask Cmdr) ALSO selected every file in the pane.
 * The fix is to resolve through the command registry:
 *
 *   eventMatchesCommand(e, 'selection.selectAll')   // exact, and follows a rebind
 *   comboMatchesCommand(combo, 'nav.down')          // same, for a pre-formatted combo
 *
 * Both live in `$lib/shortcuts`. In a window with no command registry (the viewer),
 * match locally but split on "carries ⌘/⌃/⌥" up front so a bare key can never be
 * reached by a modified one.
 *
 * ## The heuristic, and why this one
 *
 * ESLint here has no type information, so the rule CANNOT ask "is this a
 * KeyboardEvent?". Anything keyed off a variable name (`e`, `event`) or a handler
 * name (`handleKeyDown`) would be both leaky and noisy. So the rule keys off the BUG
 * SIGNATURE itself, which needs no types:
 *
 *   a REQUIRED (non-negated) modifier flag read — `.metaKey` / `.ctrlKey` / `.altKey`
 *   / `.shiftKey` — combined with a `.key` / `.code` comparison against a string
 *   LITERAL, in the same boolean expression (or in the body of the `if` that the
 *   modifier read guards), WHERE THE TEST LEAVES AT LEAST ONE OF THE FOUR MODIFIERS
 *   UNCONSTRAINED.
 *
 * That last clause is what makes "superset" precise rather than a guess. A test that
 * mentions all four flags — `(e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey &&
 * e.key === 'Enter'`, the dialogs' ⌘Enter — spells out one exact combo and is a
 * superset of nothing, so it stays quiet. `e.key === 'a' && e.metaKey` says nothing
 * about ⌥/⌃/⇧, so ⌥⌘A satisfies it. Everything else stays quiet too:
 *
 * - **Mouse / click / drag handlers** (`handleSelect(index, e.shiftKey, e.metaKey)`,
 *   `views/BriefList.svelte`, `views/FullList.svelte`, `drag/`) read modifier flags
 *   with no key comparison anywhere near them. Never flagged. This is the big
 *   false-positive source the same-expression requirement exists to avoid.
 * - **REJECTING modifiers is the safe direction, so negated reads are never flagged.**
 *   `e.key.toLowerCase() === 'w' && !e.metaKey && !e.ctrlKey && !e.altKey` is exact —
 *   it is a bare `W`, not a superset. Only a read that REQUIRES a modifier is a
 *   candidate.
 * - **Bail-out guards** (`if (e.metaKey || e.ctrlKey || e.altKey) return null` ahead of
 *   the key tests) are the other safe direction, and stay quiet because the guarded
 *   body holds no key comparison. That is what keeps the two documented class-of-key
 *   matchers clean: `pane/type-to-jump-keys.ts` (any printable character) and
 *   `pane/selection-dialog-keys.ts` (the physical Minus key) both reject every command
 *   modifier up front and then test keys separately.
 * - **Combo BUILDERS and trackers** read modifier flags to construct a string, never to
 *   match one: `lib/shortcuts/key-capture.ts` (it IS the formatter),
 *   `file-explorer/modifier-key-tracker.svelte.ts`, and the shortcut-capture UI in
 *   `settings/sections/KeyboardShortcutsSection.controller.svelte.ts`. No literal key
 *   comparison shares an expression with those reads, so none is flagged. They are
 *   also path-exempt below, so a future edit can't start tripping the rule.
 *
 * ## What it deliberately does NOT catch
 *
 * - A guard-then-branch shape, where an early `if (e.key !== 'Enter') return` is
 *   followed by modifier branches in separate statements. Same-expression pairing is
 *   what keeps the false-positive rate at zero; widening to "anywhere in the enclosing
 *   function" flags every legitimate bail-out guard in the codebase.
 * - A key compared to a VARIABLE (`e.key === key`) rather than a literal. That is a
 *   parameterized matcher, not a hardcoded combo.
 * - `switch (e.key)` dispatch, and any handler with no modifier read at all. A bare
 *   `e.key === 'Escape'` is technically a superset too, but it is the fixed-key Tier 2
 *   vocabulary the whole app is built on; flagging it would mean hundreds of reports
 *   and no signal.
 *
 * - A combo that IS exact but hardcoded (all four modifiers pinned, no registry
 *   lookup). It can't fire on the wrong keypress, so it isn't this bug; it just isn't
 *   rebindable. That's a deliberate choice in the viewer (no command registry for that
 *   window) and in a few dialogs.
 *
 * So: a clean run is NOT proof that every handler matches exactly. It is proof that
 * nobody re-introduced the ⌥⌘A shape. See `src/lib/shortcuts/CLAUDE.md`.
 *
 * Opt out per-line when a handler genuinely must read raw modifiers (a combo builder,
 * a class-of-key matcher). The reason is mandatory:
 *
 *   // eslint-disable-next-line cmdr/no-raw-key-match -- <reason>
 */

// Files that legitimately read raw modifier flags because they BUILD or TRACK a combo
// rather than match one. Nothing that dispatches on a keypress belongs here — those
// call `eventMatchesCommand` instead.
const allowedPathFragments = [
  // The canonical combo formatter itself.
  '/shortcuts/key-capture.ts',
  // Tracks which modifiers are held, to re-render drag/copy affordances.
  '/file-explorer/modifier-key-tracker.svelte.ts',
  // The Settings capture field: it must read raw modifiers to build the combo the
  // user is pressing.
  '/sections/KeyboardShortcutsSection.controller.svelte.ts',
]

const MODIFIER_PROPS = new Set(['metaKey', 'ctrlKey', 'altKey', 'shiftKey'])
const KEY_PROPS = new Set(['key', 'code'])
const EQUALITY_OPERATORS = new Set(['===', '!==', '==', '!='])

/** The expression node types that chain a modifier read into a bigger boolean test. */
const BOOLEAN_COMBINATORS = new Set(['LogicalExpression', 'UnaryExpression', 'ConditionalExpression'])

/** A string literal, or a template literal with no interpolation (still a constant). */
function isStringLiteral(node) {
  if (node.type === 'Literal') return typeof node.value === 'string'
  return node.type === 'TemplateLiteral' && node.expressions.length === 0
}

/**
 * True for a `.key` / `.code` read, including the `.toLowerCase()` / `.toUpperCase()`
 * wrapping that case-insensitive handlers use (`e.key.toLowerCase() === 'w'`), and for
 * a bare `key` / `code` identifier left by destructuring (`const { key } = event`).
 */
function isKeyRead(node) {
  if (node.type === 'CallExpression') {
    const callee = node.callee
    if (callee.type !== 'MemberExpression' || callee.computed) return false
    if (callee.property.type !== 'Identifier') return false
    if (callee.property.name !== 'toLowerCase' && callee.property.name !== 'toUpperCase') return false
    return isKeyRead(callee.object)
  }
  if (node.type === 'MemberExpression') {
    return !node.computed && node.property.type === 'Identifier' && KEY_PROPS.has(node.property.name)
  }
  return node.type === 'Identifier' && KEY_PROPS.has(node.name)
}

/** True for `e.key === 'a'` (either operand order, any equality operator). */
function isKeyLiteralComparison(node) {
  if (node.type !== 'BinaryExpression' || !EQUALITY_OPERATORS.has(node.operator)) return false
  return (isKeyRead(node.left) && isStringLiteral(node.right)) || (isStringLiteral(node.left) && isKeyRead(node.right))
}

/** Depth-first walk over an AST subtree, calling `visit` on every node. */
function walk(node, visit, seen = new Set()) {
  if (node === null || typeof node !== 'object' || seen.has(node)) return
  seen.add(node)
  if (Array.isArray(node)) {
    for (const child of node) walk(child, visit, seen)
    return
  }
  if (typeof node.type !== 'string') return
  visit(node)
  for (const [key, value] of Object.entries(node)) {
    if (key === 'parent' || value === null || typeof value !== 'object') continue
    walk(value, visit, seen)
  }
}

/** Whether a `.key`/`.code`-vs-literal comparison appears anywhere under `node`. */
function containsKeyLiteralComparison(node) {
  let found = false
  walk(node, (child) => {
    if (isKeyLiteralComparison(child)) found = true
  })
  return found
}

/**
 * The modifier flags `node` reads at all, negated or not. A test that mentions every
 * one of the four pins the combo exactly, so it isn't a superset of anything and the
 * rule stays quiet — that's what makes `(e.metaKey || e.ctrlKey) && !e.altKey &&
 * !e.shiftKey && e.key === 'Enter'` (the dialogs' ⌘Enter) legitimate.
 */
function modifiersConstrained(node) {
  const found = new Set()
  walk(node, (child) => {
    if (child.type !== 'MemberExpression' || child.computed) return
    if (child.property.type !== 'Identifier') return
    if (MODIFIER_PROPS.has(child.property.name)) found.add(child.property.name)
  })
  return found
}

/**
 * Walks up from a modifier read through the boolean expression it belongs to.
 * Returns `null` when the read is negated (an odd number of `!`s) — rejecting a
 * modifier is the SAFE direction and never a superset bug — otherwise the outermost
 * boolean expression containing it.
 */
function positiveBooleanRoot(node) {
  let current = node
  let parent = current.parent
  let negations = 0
  while (parent && BOOLEAN_COMBINATORS.has(parent.type)) {
    if (parent.type === 'UnaryExpression') {
      if (parent.operator !== '!') break
      negations += 1
    }
    // A modifier read in a ternary's branches isn't the test being built; only keep
    // walking when it's part of the boolean shape, which the test position is.
    current = parent
    parent = current.parent
  }
  return negations % 2 === 0 ? current : null
}

/** The `if` statement this expression is the test of, if any. */
function guardedIfStatement(node) {
  const parent = node.parent
  return parent && parent.type === 'IfStatement' && parent.test === node ? parent : null
}

/** Directive comments that suppress this rule, so we can demand a reason. */
const DISABLE_DIRECTIVE = /^\s*eslint-disable(?:-next-line|-line)?\s+(?<rules>.*)$/s

/** Everything before the ` -- ` separator is the rule list; everything after is the reason. */
function splitDirective(comment) {
  const separatorIndex = comment.value.indexOf('--')
  return separatorIndex === -1
    ? { directive: comment.value, reason: '' }
    : { directive: comment.value.slice(0, separatorIndex), reason: comment.value.slice(separatorIndex + 2).trim() }
}

function directiveTargetsThisRule(directive, ruleName) {
  const match = DISABLE_DIRECTIVE.exec(directive)
  if (!match) return false
  return match.groups.rules
    .split(',')
    .map((name) => name.trim())
    .includes(ruleName)
}

const RULE_NAME = 'cmdr/no-raw-key-match'

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: 'Match a whole key combo via `eventMatchesCommand`, never a hand-rolled `e.key` + modifier test.',
      recommended: true,
    },
    messages: {
      rawKeyMatch:
        'Hand-rolled key combo: `{{ modifier }}` is required alongside a literal key test. That is a modifier ' +
        'SUPERSET, so a longer combo (⌥⌘A vs ⌘A) also matches and fires this handler on its way elsewhere. Match ' +
        'the whole combo with `eventMatchesCommand(event, commandId)` (or `comboMatchesCommand`) from ' +
        '`$lib/shortcuts`, which is exact and follows a rebind. See `src/lib/shortcuts/CLAUDE.md`.',
      missingReason:
        'Explain the opt-out: write `// eslint-disable-next-line ' +
        RULE_NAME +
        ' -- <reason>`. A raw modifier read is only ever right for a combo BUILDER or a class-of-key matcher, and ' +
        'the next reader needs to know which this is.',
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename || context.getFilename() || ''
    if (allowedPathFragments.some((fragment) => filename.includes(fragment))) {
      return {}
    }
    const sourceCode = context.sourceCode || context.getSourceCode()

    return {
      MemberExpression(node) {
        if (node.computed || node.property.type !== 'Identifier') return
        if (!MODIFIER_PROPS.has(node.property.name)) return

        const root = positiveBooleanRoot(node)
        if (root === null) return

        // The combo is hand-rolled when the literal key test shares this boolean
        // expression, or sits in the body the modifier read guards
        // (`if (e.metaKey) { if (e.key === 'a') … }`).
        // The combo is hand-rolled when a literal key test shares this boolean
        // expression, or sits in the body the modifier read guards
        // (`if (e.metaKey) { if (e.key === 'a') … }`). Scan the smallest node that
        // holds both.
        const guardedIf = guardedIfStatement(root)
        let scope = null
        if (containsKeyLiteralComparison(root)) scope = root
        else if (guardedIf !== null && containsKeyLiteralComparison(guardedIf.consequent)) scope = guardedIf
        if (scope === null) return

        // …and it's a SUPERSET only if it leaves some modifier unconstrained. A test
        // that mentions all four spells out one exact combo, which is the whole point.
        if (modifiersConstrained(scope).size === MODIFIER_PROPS.size) return

        context.report({
          node,
          messageId: 'rawKeyMatch',
          data: { modifier: node.property.name },
        })
      },

      // A suppression without a stated reason is reported on the COMMENT's own line,
      // which `eslint-disable-next-line` doesn't cover — so the demand can't be
      // silenced by the very directive it's complaining about.
      'Program:exit'() {
        for (const comment of sourceCode.getAllComments()) {
          const { directive, reason } = splitDirective(comment)
          if (!directiveTargetsThisRule(directive, RULE_NAME)) continue
          if (reason.length > 0) continue
          context.report({ loc: comment.loc, messageId: 'missingReason' })
        }
      },
    }
  },
}
