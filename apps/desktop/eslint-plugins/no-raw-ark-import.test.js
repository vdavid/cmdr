import { RuleTester } from 'eslint'
import rule from './no-raw-ark-import.js'

// Flat-config RuleTester (ESLint 9+): module source, latest ECMA. RuleTester
// auto-detects Vitest's `describe`/`it` globals and emits one test per case, so
// `run` is called at the top level (it can't be nested inside our own `it`).
const ruleTester = new RuleTester({
  languageOptions: { ecmaVersion: 'latest', sourceType: 'module' },
})

ruleTester.run('no-raw-ark-import', rule, {
  valid: [
    // Any house primitive under `lib/ui/` may wrap an Ark primitive — exempt by path.
    {
      code: `import { NumberInput } from '@ark-ui/svelte/number-input'`,
      filename: 'src/lib/ui/NumberInput.svelte',
    },
    {
      code: `import { Switch } from '@ark-ui/svelte/switch'`,
      filename: 'src/lib/ui/Switch.svelte',
    },
    // The three sanctioned settings-local wrappers are exempt by explicit path.
    {
      code: `import { Slider } from '@ark-ui/svelte/slider'`,
      filename: 'src/lib/settings/components/SettingSlider.svelte',
    },
    {
      code: `import { NumberInput } from '@ark-ui/svelte/number-input'`,
      filename: 'src/lib/settings/components/SettingNumberInput.svelte',
    },
    {
      code: `import { Slider } from '@ark-ui/svelte/slider'`,
      filename: 'src/lib/settings/sections/MediaIndexImportanceSlider.svelte',
    },
    // Importing a house wrapper is the intended path in feature code.
    {
      code: `import Switch from '$lib/ui/Switch.svelte'`,
      filename: 'src/lib/settings/sections/AdvancedSection.svelte',
    },
    // Unrelated imports are never touched, even from a section file.
    {
      code: `import { onMount } from 'svelte'`,
      filename: 'src/lib/settings/sections/AdvancedSection.svelte',
    },
  ],
  invalid: [
    // Direct Ark subpath import in a section (the AdvancedSection regression).
    {
      code: `import { NumberInput } from '@ark-ui/svelte/number-input'`,
      filename: 'src/lib/settings/sections/AdvancedSection.svelte',
      errors: [{ messageId: 'rawArk' }],
    },
    // The bare `@ark-ui/svelte` entry point is equally off-limits.
    {
      code: `import { Slider } from '@ark-ui/svelte'`,
      filename: 'src/lib/file-explorer/FilePane.svelte',
      errors: [{ messageId: 'rawArk' }],
    },
  ],
})
