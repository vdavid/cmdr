/**
 * ESLint rule: ban writing to the explorer store's surface from outside the
 * store module.
 *
 * Rationale (invariant A2): the dual-pane explorer's navigation + UI-chrome
 * state lives in one module store (`explorer-state.svelte.ts`). Every store
 * field has exactly one named mutator, all inside that module. The store's
 * `$state` is module-private — `createExplorerState()` closes over `$state`
 * locals and exposes only getters and mutators, so the *only* way to change
 * store state is to call a named mutator. This rule keeps that wall standing
 * after the component boundary comes down: discipline that isn't enforced decays.
 *
 * ## What this rule catches
 *
 * Assignment to a property of the store object: `explorerState.x = …`,
 * `explorerState.getFocusedPane = …` (monkey-patching a getter/mutator),
 * compound assignment (`explorerState.x += 1`), and `++/--` on a store property.
 * These are the realistic decay vectors once someone reaches for "just set the
 * field directly" instead of routing through a mutator, or papers over the
 * private-state wall by reassigning an exported member.
 *
 * The store object is recognized by the local binding imported from the store
 * module: the default app singleton (`import { explorerState } from
 * './explorer-state.svelte'`) and any instance named via the factory
 * (`const s = createExplorerState()`). Both expose the same closed surface.
 *
 * ## What this rule deliberately does NOT catch (and why)
 *
 * - **Direct `$state` field writes inside the store.** Invariant A1 already makes
 *   this structurally impossible from the outside: nothing writable is exported,
 *   so there's no symbol an external module could assign to a backing field. A
 *   type-aware rule chasing "is this property a store field" would be fragile and
 *   redundant; the assignment ban above covers the only expressible attack.
 * - **Re-exporting / aliasing a mutator** (`export const setFocus =
 *   explorerState.setFocusedPane`). A re-exported mutator is still a legitimate
 *   named-mutator call at the end of the chain, and forbidding aliasing would
 *   false-positive on legitimate read wrappers (`focused-pane-reads.ts` wraps the
 *   getters). The A2 contract is "one mutator per field, inside the store"; an
 *   alias doesn't add a writer, it just renames the call. Detecting "a hidden
 *   write" here can't be done robustly with AST analysis, so it's left to review.
 *
 * The store file itself, test files, and `/test/` dirs are exempt (mirrors
 * `no-raw-tauri-invoke`): the store assigns its own backing `$state`, and tests
 * may construct and poke instances.
 *
 * Opt out per-line if you really must:
 *
 *   // eslint-disable-next-line cmdr/no-explorer-state-writes -- <reason>
 */

// Path fragments that opt a file out entirely. The store file owns its own
// writes; tests construct and exercise instances directly.
const allowedPathFragments = ['/explorer-state.svelte.ts', '.test.', '/test/']

// The store module specifier (with and without the `.svelte` runes suffix) and
// the factory that mints fresh instances.
const STORE_MODULE_RE = /(^|\/)explorer-state\.svelte(\.ts)?$/
const FACTORY_NAME = 'createExplorerState'
const SINGLETON_NAME = 'explorerState'

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Change explorer-store state only through its named mutators (`setFocusedPane`, `setShowHiddenFiles`, …); never assign to a store property.',
      recommended: true,
    },
    messages: {
      storeWrite:
        "Don't assign to `{{ name }}.{{ property }}`. The explorer store's state is module-private (invariant A1) " +
        'and every field has exactly one named mutator inside `explorer-state.svelte.ts` (A2). ' +
        'Call a mutator (`setFocusedPane`, `setShowHiddenFiles`, `toggleHiddenFiles`, `setLeftPaneWidthPercent`, `setTabMgr`) instead. ' +
        'See `pane/CLAUDE.md` § "Explorer store".',
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename || context.getFilename() || ''
    if (allowedPathFragments.some((fragment) => filename.includes(fragment))) {
      return {}
    }

    // Local names bound to a store instance: the imported singleton, plus any
    // `const x = createExplorerState()`. Both carry the closed write surface.
    const storeBindings = new Set()

    return {
      ImportDeclaration(node) {
        if (typeof node.source.value !== 'string' || !STORE_MODULE_RE.test(node.source.value)) return
        for (const spec of node.specifiers) {
          if (
            spec.type === 'ImportSpecifier' &&
            spec.imported.type === 'Identifier' &&
            spec.imported.name === SINGLETON_NAME
          ) {
            storeBindings.add(spec.local.name)
          }
        }
      },
      VariableDeclarator(node) {
        // `const s = createExplorerState()` — track the local as a store instance.
        if (
          node.id.type === 'Identifier' &&
          node.init?.type === 'CallExpression' &&
          node.init.callee.type === 'Identifier' &&
          node.init.callee.name === FACTORY_NAME
        ) {
          storeBindings.add(node.id.name)
        }
      },
      AssignmentExpression(node) {
        reportIfStoreProperty(context, storeBindings, node.left, node)
      },
      UpdateExpression(node) {
        // `explorerState.x++` / `--explorerState.x`
        reportIfStoreProperty(context, storeBindings, node.argument, node)
      },
    }
  },
}

/**
 * Report when `target` is a member access (`store.prop`) on a tracked store
 * binding. `reportNode` is the whole assignment/update so the squiggle covers it.
 */
function reportIfStoreProperty(context, storeBindings, target, reportNode) {
  if (
    target.type !== 'MemberExpression' ||
    target.object.type !== 'Identifier' ||
    !storeBindings.has(target.object.name)
  ) {
    return
  }
  const property =
    !target.computed && target.property.type === 'Identifier'
      ? target.property.name
      : target.property.type === 'Literal'
        ? String(target.property.value)
        : '…'
  context.report({
    node: reportNode,
    messageId: 'storeWrite',
    data: { name: target.object.name, property },
  })
}
