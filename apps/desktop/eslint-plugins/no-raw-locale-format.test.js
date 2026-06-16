import { RuleTester } from 'eslint'
import rule from './no-raw-locale-format.js'

const ruleTester = new RuleTester({
  languageOptions: { ecmaVersion: 'latest', sourceType: 'module' },
})

ruleTester.run('no-raw-locale-format', rule, {
  valid: [
    // Routing through the central helpers is the intended path.
    {
      code: `import { formatInteger } from '$lib/intl/number-format'; const s = formatInteger(n)`,
      filename: 'src/lib/ui/LoadingIcon.svelte',
    },
    // The intl layer itself is exempt: it's where the formatters live.
    {
      code: `const f = new Intl.NumberFormat(locale, options)`,
      filename: 'src/lib/intl/number-format.ts',
    },
    // The central date formatter is exempt.
    {
      code: `const f = new Intl.DateTimeFormat(getLocale(), opts)`,
      filename: 'src/lib/settings/format-utils.ts',
    },
    // The calendar helper legitimately builds locale-aware names.
    {
      code: `const f = new Intl.DateTimeFormat(language, { weekday: 'long' })`,
      filename: 'src/lib/query-ui/filter-chips/filter-popover-helpers.ts',
    },
    // Intl.Segmenter / Intl.Locale are not formatters.
    {
      code: `const s = new Intl.Segmenter(undefined, { granularity: 'word' })`,
      filename: 'src/routes/viewer/viewer-word.ts',
    },
    {
      code: `const l = new Intl.Locale(language)`,
      filename: 'src/lib/query-ui/filter-chips/filter-popover-helpers.ts',
    },
  ],
  invalid: [
    {
      code: `const s = n.toLocaleString('en-US')`,
      filename: 'src/lib/file-explorer/selection/selection-info-utils.ts',
      errors: [{ messageId: 'rawToLocaleString' }],
    },
    {
      code: `const s = count.toLocaleString()`,
      filename: 'src/lib/query-ui/QueryResults.svelte',
      errors: [{ messageId: 'rawToLocaleString' }],
    },
    {
      code: `const f = new Intl.NumberFormat('en-US')`,
      filename: 'src/lib/foo/bar.ts',
      errors: [{ messageId: 'rawIntlFormatter' }],
    },
    {
      code: `const f = new Intl.DateTimeFormat(undefined, {})`,
      filename: 'src/lib/foo/bar.ts',
      errors: [{ messageId: 'rawIntlFormatter' }],
    },
  ],
})
