import { RuleTester } from 'eslint'
import * as svelteParser from 'svelte-eslint-parser'
import rule from './prefer-ui-primitive.js'

// Flat-config RuleTester (ESLint 9+) with the Svelte parser, since the rule
// visits Svelte template AST nodes. RuleTester auto-detects Vitest's
// `describe`/`it` globals, so `run` is called at the top level.
const ruleTester = new RuleTester({
  languageOptions: { parser: svelteParser, ecmaVersion: 'latest', sourceType: 'module' },
})

ruleTester.run('prefer-ui-primitive', rule, {
  valid: [
    // A text input has no primitive replacement.
    {
      code: `<input type="text" bind:value={name} />`,
      filename: 'src/lib/whatever/Form.svelte',
    },
    // A typeless input defaults to text: out of scope.
    {
      code: `<input bind:value={name} />`,
      filename: 'src/lib/whatever/Form.svelte',
    },
    // Dynamic type can't be resolved statically, so we don't flag it.
    {
      code: `<input type={kind} bind:value={val} />`,
      filename: 'src/lib/whatever/Form.svelte',
    },
    // Rendering the house primitives (components, kind !== 'html') is the
    // intended path and must never be flagged.
    {
      code: `<Checkbox bind:checked={on} />`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    {
      code: `<RadioGroup {items} bind:value={choice} />`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    {
      code: `<Select {items} bind:value={choice} />`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    {
      code: `<ModalDialog>x</ModalDialog>`,
      filename: 'src/lib/whatever/Dialog.svelte',
    },
    {
      code: `<ProgressBar value={0.5} />`,
      filename: 'src/lib/whatever/Bar.svelte',
    },
    // An unrelated native element is never touched.
    {
      code: `<button type="button">go</button>`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    // (The per-element opt-out comment is exercised end-to-end by the real
    // eslint config against the bespoke source sites, not here: RuleTester
    // registers the rule under a `rule-to-test/*` id, so a `cmdr/*` disable
    // directive can't match inside the harness.)
  ],
  invalid: [
    // Raw checkbox → Checkbox.
    {
      code: `<input type="checkbox" bind:checked={on} />`,
      filename: 'src/lib/whatever/Row.svelte',
      errors: [
        {
          messageId: 'preferPrimitive',
          data: { control: '<input type="checkbox">', primitive: 'Checkbox', path: '$lib/ui/Checkbox.svelte' },
        },
      ],
    },
    // Raw radio → RadioGroup.
    {
      code: `<input type="radio" name="c" value="a" />`,
      filename: 'src/lib/whatever/Row.svelte',
      errors: [
        {
          messageId: 'preferPrimitive',
          data: { control: '<input type="radio">', primitive: 'RadioGroup', path: '$lib/ui/RadioGroup.svelte' },
        },
      ],
    },
    // Single-quoted static type is still a static literal.
    {
      code: `<input type='checkbox' bind:checked={on} />`,
      filename: 'src/lib/whatever/Row.svelte',
      errors: [
        {
          messageId: 'preferPrimitive',
          data: { control: '<input type="checkbox">', primitive: 'Checkbox', path: '$lib/ui/Checkbox.svelte' },
        },
      ],
    },
    // Raw select → Select (no type predicate needed).
    {
      code: `<select bind:value={choice}><option>a</option></select>`,
      filename: 'src/lib/whatever/Row.svelte',
      errors: [
        {
          messageId: 'preferPrimitive',
          data: { control: '<select>', primitive: 'Select', path: '$lib/ui/Select.svelte' },
        },
      ],
    },
    // Raw dialog → ModalDialog (pure regression guard; none exist today).
    {
      code: `<dialog open>x</dialog>`,
      filename: 'src/lib/whatever/Dialog.svelte',
      errors: [
        {
          messageId: 'preferPrimitive',
          data: { control: '<dialog>', primitive: 'ModalDialog', path: '$lib/ui/ModalDialog.svelte' },
        },
      ],
    },
    // Raw progress → ProgressBar (pure regression guard; none exist today).
    {
      code: `<progress value={0.5}></progress>`,
      filename: 'src/lib/whatever/Bar.svelte',
      errors: [
        {
          messageId: 'preferPrimitive',
          data: { control: '<progress>', primitive: 'ProgressBar', path: '$lib/ui/ProgressBar.svelte' },
        },
      ],
    },
  ],
})
