import { RuleTester } from 'eslint'
import rule from './no-explorer-state-writes.js'

// Flat-config RuleTester (ESLint 9+): module source, latest ECMA. RuleTester
// auto-detects Vitest's `describe`/`it` globals and emits one test per case, so
// `run` is called at the top level (it can't be nested inside our own `it`).
const ruleTester = new RuleTester({
  languageOptions: { ecmaVersion: 'latest', sourceType: 'module' },
})

ruleTester.run('no-explorer-state-writes', rule, {
  valid: [
    // Calling a named mutator is the whole point — never reported.
    {
      code: `import { explorerState } from './explorer-state.svelte'\nexplorerState.setFocusedPane('left')`,
      filename: 'src/lib/file-explorer/pane/DualPaneExplorer.svelte.ts',
    },
    // Reads through getters are fine.
    {
      code: `import { explorerState } from './explorer-state.svelte'\nconst p = explorerState.getFocusedPane()`,
      filename: 'src/lib/file-explorer/pane/focused-pane-reads.ts',
    },
    // Assigning to a property of some *other* object is unrelated.
    {
      code: `import { explorerState } from './explorer-state.svelte'\nconst obj = { x: 0 }\nobj.x = 1\nexplorerState.toggleHiddenFiles()`,
      filename: 'src/lib/file-explorer/pane/DualPaneExplorer.svelte.ts',
    },
    // A local not bound to the store is unrelated even with the same shape.
    {
      code: `const explorerState = { x: 0 }\nexplorerState.x = 1`,
      filename: 'src/lib/file-explorer/pane/other.ts',
    },
    // The store file itself may assign its own surface — exempt by path.
    {
      code: `const explorerState = makeIt()\nexplorerState.focusedPane = 'right'`,
      filename: 'src/lib/file-explorer/pane/explorer-state.svelte.ts',
    },
    // Test files are exempt by path (they construct and poke instances).
    {
      code: `import { explorerState } from './explorer-state.svelte'\nexplorerState.focusedPane = 'right'`,
      filename: 'src/lib/file-explorer/pane/explorer-state.test.ts',
    },
    // A factory instance read is fine.
    {
      code: `import { createExplorerState } from './explorer-state.svelte'\nconst s = createExplorerState()\nconst p = s.getFocusedPane()`,
      filename: 'src/lib/file-explorer/pane/consumer.ts',
    },
  ],
  invalid: [
    // Direct field assignment on the imported singleton.
    {
      code: `import { explorerState } from './explorer-state.svelte'\nexplorerState.focusedPane = 'right'`,
      filename: 'src/lib/file-explorer/pane/DualPaneExplorer.svelte.ts',
      errors: [{ messageId: 'storeWrite' }],
    },
    // Monkey-patching a mutator/getter.
    {
      code: `import { explorerState } from './explorer-state.svelte'\nexplorerState.setFocusedPane = () => {}`,
      filename: 'src/lib/file-explorer/pane/DualPaneExplorer.svelte.ts',
      errors: [{ messageId: 'storeWrite' }],
    },
    // Compound assignment.
    {
      code: `import { explorerState } from './explorer-state.svelte'\nexplorerState.leftPaneWidthPercent += 10`,
      filename: 'src/lib/file-explorer/pane/DualPaneExplorer.svelte.ts',
      errors: [{ messageId: 'storeWrite' }],
    },
    // Update expression (++/--).
    {
      code: `import { explorerState } from './explorer-state.svelte'\nexplorerState.leftPaneWidthPercent++`,
      filename: 'src/lib/file-explorer/pane/DualPaneExplorer.svelte.ts',
      errors: [{ messageId: 'storeWrite' }],
    },
    // A factory-minted instance carries the same closed surface.
    {
      code: `import { createExplorerState } from './explorer-state.svelte'\nconst s = createExplorerState()\ns.focusedPane = 'right'`,
      filename: 'src/lib/file-explorer/pane/consumer.ts',
      errors: [{ messageId: 'storeWrite' }],
    },
    // Aliased import binding is still tracked.
    {
      code: `import { explorerState as store } from './explorer-state.svelte'\nstore.showHiddenFiles = false`,
      filename: 'src/lib/file-explorer/pane/consumer.ts',
      errors: [{ messageId: 'storeWrite' }],
    },
  ],
})
