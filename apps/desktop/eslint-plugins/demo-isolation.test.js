import { RuleTester } from 'eslint'
import rule from './demo-isolation.js'

const ruleTester = new RuleTester({
  languageOptions: { ecmaVersion: 'latest', sourceType: 'module' },
})

ruleTester.run('demo-isolation', rule, {
  valid: [
    // The one sanctioned app-side import.
    { code: `import '#lib/demo/boot.js'`, filename: 'src/hooks.client.ts' },
    // The demo itself may use its flag and its own modules.
    { code: `if (__CMDR_DEMO_BUILD__) install()`, filename: 'src/lib/demo/boot.ts' },
    { code: `import { demoEntries } from './demo-tree.js'`, filename: 'src/lib/demo/listing.ts' },
    // The global's declaration.
    { code: `const __CMDR_DEMO_BUILD__ = false`, filename: 'src/app.d.ts' },
    // Unrelated names and paths stay untouched.
    { code: `import { x } from '#lib/demos-and-more/y.js'`, filename: 'src/lib/file-explorer/a.ts' },
    { code: `if (__CMDR_E2E_BUILD__) run()`, filename: 'src/lib/intl/a.ts' },
  ],
  invalid: [
    {
      code: `if (__CMDR_DEMO_BUILD__) skip()`,
      filename: 'src/lib/file-explorer/a.ts',
      errors: [{ messageId: 'demoFlag' }],
    },
    {
      code: `import { demoEntries } from '#lib/demo/demo-tree.js'`,
      filename: 'src/lib/file-explorer/a.ts',
      errors: [{ messageId: 'demoImport' }],
    },
    { code: `import '../demo/boot.js'`, filename: 'src/lib/ui/a.ts', errors: [{ messageId: 'demoImport' }] },
    {
      code: `export { noteUnanswered } from '../lib/demo/unanswered.js'`,
      filename: 'src/routes/a.ts',
      errors: [{ messageId: 'demoImport' }],
    },
    { code: `void import('#lib/demo/listing.js')`, filename: 'src/routes/a.ts', errors: [{ messageId: 'demoImport' }] },
    { code: `const on = __CMDR_DEMO_BUILD__`, filename: 'src/lib/app-mode.ts', errors: [{ messageId: 'demoFlag' }] },
    // `hooks.client.ts` may import the demo's boot module, but not branch on the flag.
    { code: `if (__CMDR_DEMO_BUILD__) x()`, filename: 'src/hooks.client.ts', errors: [{ messageId: 'demoFlag' }] },
  ],
})
