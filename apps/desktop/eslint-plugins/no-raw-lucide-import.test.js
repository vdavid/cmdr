import { RuleTester } from 'eslint'
import rule from './no-raw-lucide-import.js'

// Flat-config RuleTester (ESLint 9+): module source, latest ECMA. RuleTester
// auto-detects Vitest's `describe`/`it` globals and emits one test per case, so
// `run` is called at the top level (it can't be nested inside our own `it`).
const ruleTester = new RuleTester({
  languageOptions: { ecmaVersion: 'latest', sourceType: 'module' },
})

ruleTester.run('no-raw-lucide-import', rule, {
  valid: [
    // The registry IS the one place `~icons/*` is imported — exempt by path.
    {
      code: `import IconTriangleAlert from '~icons/lucide/triangle-alert'`,
      filename: 'src/lib/ui/icons/icon-map.ts',
    },
    // A custom-glyph component colocated in the registry dir is also exempt.
    {
      code: `import IconFoo from '~icons/lucide/foo'`,
      filename: 'src/lib/ui/icons/EjectIcon.svelte',
    },
    // Feature code importing the shared Icon component is the intended path.
    {
      code: `import Icon from '$lib/ui/Icon.svelte'`,
      filename: 'src/lib/file-explorer/selection/FileIcon.svelte',
    },
    // Importing from the registry module (not `~icons/*`) is fine anywhere.
    {
      code: `import { ICON_COMPONENTS } from '$lib/ui/icons/icon-map'`,
      filename: 'src/lib/file-explorer/selection/FileIcon.svelte',
    },
    // Unrelated imports are never touched.
    {
      code: `import { onMount } from 'svelte'`,
      filename: 'src/lib/ui/Combobox.svelte',
    },
  ],
  invalid: [
    // Direct Lucide import in feature code.
    {
      code: `import IconSearch from '~icons/lucide/search'`,
      filename: 'src/lib/query-ui/QueryBar.svelte',
      errors: [{ messageId: 'rawLucide' }],
    },
    // Any other Iconify set is equally off-registry.
    {
      code: `import IconFoo from '~icons/mdi/foo'`,
      filename: 'src/lib/settings/components/SettingRow.svelte',
      errors: [{ messageId: 'rawLucide' }],
    },
  ],
})
