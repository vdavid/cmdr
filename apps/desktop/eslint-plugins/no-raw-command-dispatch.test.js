import { RuleTester } from 'eslint'
import rule from './no-raw-command-dispatch.js'

// Flat-config RuleTester (ESLint 9+): module source, latest ECMA. RuleTester
// auto-detects Vitest's `describe`/`it` globals and emits one test per case, so
// `run` is called at the top level (it can't be nested inside our own `it`).
const ruleTester = new RuleTester({
  languageOptions: { ecmaVersion: 'latest', sourceType: 'module' },
})

ruleTester.run('no-raw-command-dispatch', rule, {
  valid: [
    // A `CommandId`-typed local is the whole point — never reported.
    {
      code: `handleCommandExecute(commandId, ctx)`,
      filename: 'src/routes/(main)/+page.svelte.ts',
    },
    // A member access (a narrowed / forwarded value) is fine.
    {
      code: `dispatch(args.id)`,
      filename: 'src/routes/(main)/mcp-listeners.ts',
    },
    // Forwarding a prop reference (not a call with a literal) is fine.
    {
      code: `onExecute(id)`,
      filename: 'src/lib/command-palette/CommandPalette.svelte.ts',
    },
    // A literal first arg to some *other* function is unrelated.
    {
      code: `log.info('file.rename')`,
      filename: 'src/routes/(main)/command-dispatch.ts',
    },
    // The registry file may hold command-id literals — exempt by path.
    {
      code: `dispatch('file.rename')`,
      filename: 'src/lib/commands/command-registry.ts',
    },
    // The id tuple may hold command-id literals — exempt by path.
    {
      code: `handleCommandExecute('file.rename')`,
      filename: 'src/lib/commands/command-ids.ts',
    },
    // Test files dispatch literal ids on purpose — exempt by path.
    {
      code: `handleCommandExecute('file.rename', ctx)`,
      filename: 'src/routes/(main)/command-dispatch.test.ts',
    },
    // A non-dispatch member call with a literal is unrelated.
    {
      code: `store.subscribe('file.rename')`,
      filename: 'src/lib/whatever.ts',
    },
  ],
  invalid: [
    // Bare-identifier dispatch of a literal.
    {
      code: `handleCommandExecute('file.rename')`,
      filename: 'src/routes/(main)/+page.svelte.ts',
      errors: [{ messageId: 'rawDispatch' }],
    },
    // Literal id with a trailing ctx argument.
    {
      code: `dispatchCommand('file.rename', ctx)`,
      filename: 'src/routes/(main)/+page.svelte.ts',
      errors: [{ messageId: 'rawDispatch' }],
    },
    // `dispatch(...)` with a literal.
    {
      code: `dispatch('sort.byName')`,
      filename: 'src/routes/(main)/mcp-listeners.ts',
      errors: [{ messageId: 'rawDispatch' }],
    },
    // Member-call dispatch entry point (`ctx.onCommand('…')`).
    {
      code: `ctx.onCommand('selection.selectFiles')`,
      filename: 'src/lib/file-explorer/pane/FilePane.svelte.ts',
      errors: [{ messageId: 'rawDispatch' }],
    },
    // Template literal with no expressions is still a constant string.
    {
      code: 'handleCommandExecute(`file.rename`)',
      filename: 'src/routes/(main)/+page.svelte.ts',
      errors: [{ messageId: 'rawDispatch' }],
    },
  ],
})
