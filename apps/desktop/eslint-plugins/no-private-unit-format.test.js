import { RuleTester } from 'eslint'
import rule from './no-private-unit-format.js'

const ruleTester = new RuleTester({
  languageOptions: { ecmaVersion: 'latest', sourceType: 'module' },
})

ruleTester.run('no-private-unit-format', rule, {
  valid: [
    // Routing through `$lib/units` is the intended path.
    {
      code: `import { formatByteSize } from '$lib/units'; const s = formatByteSize(n)`,
      filename: 'src/lib/file-operations/queue/QueueRow.svelte',
    },
    // The units module itself is exempt: it's where the ladder lives.
    {
      code: `const base = format === 'binary' ? 1024 : 1000; const v = n / base ** 2`,
      filename: 'src/lib/units/byte-size.ts',
    },
    // The size-tier layer is exempt (it consumes the ladder from `$lib/units`).
    {
      code: `const tier = bytes / 1024`,
      filename: 'src/lib/file-explorer/selection/selection-info-utils.ts',
    },
    // Tests legitimately spell out `1024 * 1024` in fixtures.
    {
      code: `expect(formatByteSize(1024 * 1024)).toBe('1.00 MB')`,
      filename: 'src/lib/units/some.test.ts',
    },
    // A bare comparison against 1024 is a threshold, not a conversion.
    {
      code: `function f(chunk) { if (chunk < 1024) return true; return false }`,
      filename: 'src/lib/file-operations/transfer/x.ts',
    },
    // A ladder value that isn't an operand of `*` / `/` / `**`.
    {
      code: `const sizes = [1024, 1000]`,
      filename: 'src/lib/file-operations/transfer/x.ts',
    },
    // A formatter for something that isn't a unit.
    {
      code: `function formatPath(p) { return p.replace(/\\/+$/, '') }`,
      filename: 'src/lib/file-operations/queue/QueueRow.svelte',
    },
    // A formatter-shaped NAME that just delegates to `$lib/units` keeps its name.
    {
      code: `function formatDbSize(b) { return b === null ? 'N/A' : formatByteSize(b) }`,
      filename: 'src/routes/debug/DebugDriveIndexPanel.svelte',
    },
    // Base-1000 arithmetic is milliseconds far more often than kilobytes.
    {
      code: `const elapsedSeconds = elapsedMs / 1000`,
      filename: 'src/lib/file-operations/scan-throughput.ts',
    },
  ],
  invalid: [
    // The exact shape of the four formatters this rule exists to prevent.
    {
      code: `function formatBytes(b) { if (b < 1024) return b + ' B'; return (b / 1024).toFixed(1) + ' KB' }`,
      filename: 'src/lib/tauri-commands/write-operations.ts',
      errors: [{ messageId: 'privateFormatter' }, { messageId: 'privateLadder' }],
    },
    // Ladder arithmetic on its own, even without a telltale name.
    {
      code: `const mb = value / (1024 * 1024)`,
      filename: 'src/routes/debug/DebugDriveIndexPanel.svelte',
      errors: [{ messageId: 'privateLadder' }, { messageId: 'privateLadder' }],
    },
    // A pre-multiplied ladder constant.
    {
      code: `const mb = value / 1048576`,
      filename: 'src/lib/x.ts',
      errors: [{ messageId: 'privateLadder' }],
    },
    // Arrow-function formatters count as declarations.
    {
      code: `const formatSpeed = (r) => r + ' B/s'`,
      filename: 'src/lib/file-operations/transfer/x.ts',
      errors: [{ messageId: 'privateFormatter' }],
    },
    // Durations and ETAs are in scope, not just sizes.
    {
      code: `function formatEta(s) { return s + ' sec' }`,
      filename: 'src/lib/file-operations/queue/x.ts',
      errors: [{ messageId: 'privateFormatter' }],
    },
  ],
})
