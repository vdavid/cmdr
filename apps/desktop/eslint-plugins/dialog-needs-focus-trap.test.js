import { RuleTester } from 'eslint'
import * as svelteParser from 'svelte-eslint-parser'
import rule from './dialog-needs-focus-trap.js'

// Flat-config RuleTester (ESLint 9+) with the Svelte parser, since the rule
// visits Svelte template AST nodes. RuleTester auto-detects Vitest's
// `describe`/`it` globals, so `run` is called at the top level.
const ruleTester = new RuleTester({
  languageOptions: { parser: svelteParser, ecmaVersion: 'latest', sourceType: 'module' },
})

ruleTester.run('dialog-needs-focus-trap', rule, {
  valid: [
    // Trapped dialog: the rule's happy path.
    {
      code: `<div role="dialog" use:trapFocus={{ onEscape: close }}>x</div>`,
      filename: 'src/lib/whatever/Dialog.svelte',
    },
    // Trapped alertdialog, parameterless action form (onboarding-wizard style).
    {
      code: `<div role="alertdialog" use:trapFocus>x</div>`,
      filename: 'src/lib/whatever/Alert.svelte',
    },
    // Non-dialog roles are out of scope.
    {
      code: `<div role="listbox">x</div>`,
      filename: 'src/lib/whatever/List.svelte',
    },
    // Dynamic role can't be resolved statically (ModalDialog's `{role}` prop).
    {
      code: `<div {role} use:somethingElse>x</div>`,
      filename: 'src/lib/ui/ModalDialog.svelte',
    },
    // No role attribute at all.
    {
      code: `<div class="overlay">x</div>`,
      filename: 'src/lib/whatever/Plain.svelte',
    },
  ],
  invalid: [
    // Untrapped dialog: the command-palette lockout class of bug.
    {
      code: `<div role="dialog" aria-modal="true">x</div>`,
      filename: 'src/lib/whatever/Dialog.svelte',
      errors: [{ messageId: 'missingTrap' }],
    },
    // Untrapped alertdialog.
    {
      code: `<div role="alertdialog">x</div>`,
      filename: 'src/lib/whatever/Alert.svelte',
      errors: [{ messageId: 'missingTrap' }],
    },
    // A different action doesn't satisfy the requirement.
    {
      code: `<div role="dialog" use:tooltip={'hi'}>x</div>`,
      filename: 'src/lib/whatever/Dialog.svelte',
      errors: [{ messageId: 'missingTrap' }],
    },
  ],
})
