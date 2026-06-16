import { RuleTester } from 'eslint'
import * as svelteParser from 'svelte-eslint-parser'
import rule from './no-raw-user-facing-string.js'

// Two RuleTesters: the default (TS) parser for `.ts` sink cases (`addToast`),
// and the Svelte parser for markup sinks (attributes + JSX text). RuleTester
// auto-detects Vitest's `describe`/`it`, so `run` is called at the top level.
const tsTester = new RuleTester({
  languageOptions: { ecmaVersion: 'latest', sourceType: 'module' },
})
const svelteTester = new RuleTester({
  languageOptions: { parser: svelteParser, ecmaVersion: 'latest', sourceType: 'module' },
})

// The rule is scoped to an enforced area (the `transfer` dir in M1) MINUS the
// not-yet-migrated dialog files. The migrated pilot composer is enforced; a
// hypothetical migrated transfer component stands in for the enforced `.svelte`
// surface (the real dialogs are still excluded until their M2 tranche).
const TRANSFER_TS = 'src/lib/file-operations/transfer/transfer-complete-toast.ts'
const TRANSFER_SVELTE = 'src/lib/file-operations/transfer/CompletedTransferBanner.svelte'
// An excluded (un-migrated) transfer dialog: still-raw copy must NOT flag yet.
const EXCLUDED_TRANSFER_SVELTE = 'src/lib/file-operations/transfer/TransferDialog.svelte'
// A non-enforced area: identical raw strings here must NOT flag yet.
const OTHER_SVELTE = 'src/lib/file-explorer/pane/FilePane.svelte'

tsTester.run('no-raw-user-facing-string (ts sinks)', rule, {
  valid: [
    // A `t()` result passed to addToast is the intended path.
    {
      code: `addToast(t('transfer.trash', { count: 1 }))`,
      filename: TRANSFER_TS,
    },
    // A non-string variable into addToast is fine.
    {
      code: `addToast(message)`,
      filename: TRANSFER_TS,
    },
    // A raw addToast string OUTSIDE an enforced area is not flagged yet.
    {
      code: `addToast('Copied 3 files')`,
      filename: 'src/lib/search/search-toast.ts',
    },
    // A string literal that isn't a recognized sink (a log line) is ignored.
    {
      code: `log.info('starting transfer')`,
      filename: TRANSFER_TS,
    },
  ],
  invalid: [
    // Raw user-facing string passed to addToast in an enforced area.
    {
      code: `addToast('Copied 3 files')`,
      filename: TRANSFER_TS,
      errors: [{ messageId: 'rawUserFacingString' }],
    },
    // addToast with a content + options object: the content arg still flags.
    {
      code: `addToast('Move complete', { level: 'success' })`,
      filename: TRANSFER_TS,
      errors: [{ messageId: 'rawUserFacingString' }],
    },
  ],
})

svelteTester.run('no-raw-user-facing-string (svelte sinks)', rule, {
  valid: [
    // A localized prop value (an expression) is the intended path.
    {
      code: `<button title={t('transfer.cancel')}><Icon name="x" /></button>`,
      filename: TRANSFER_SVELTE,
    },
    // A raw markup string in a NON-enforced area is not flagged yet.
    {
      code: `<button title="Cancel"><Icon name="x" /></button>`,
      filename: OTHER_SVELTE,
    },
    // A non-sink attribute literal (a CSS class) is ignored even when enforced.
    {
      code: `<button class="primary"><Icon name="x" /></button>`,
      filename: TRANSFER_SVELTE,
    },
    // An excluded (un-migrated) transfer dialog: still-raw copy must NOT flag.
    {
      code: `<button title="Cancel transfer"><Icon name="x" /></button>`,
      filename: EXCLUDED_TRANSFER_SVELTE,
    },
  ],
  invalid: [
    // Raw `title` attribute in an enforced area.
    {
      code: `<button title="Cancel transfer"><Icon name="x" /></button>`,
      filename: TRANSFER_SVELTE,
      errors: [{ messageId: 'rawUserFacingString' }],
    },
    // Raw `aria-label` attribute in an enforced area.
    {
      code: `<button aria-label="Close dialog"><Icon name="x" /></button>`,
      filename: TRANSFER_SVELTE,
      errors: [{ messageId: 'rawUserFacingString' }],
    },
    // Raw text node in an enforced area's markup.
    {
      code: `<p>Transfer complete</p>`,
      filename: TRANSFER_SVELTE,
      errors: [{ messageId: 'rawUserFacingString' }],
    },
  ],
})
