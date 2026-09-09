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
    // Dynamic type can't be resolved statically, so we don't flag it.
    {
      code: `<input type={kind} bind:value={val} />`,
      filename: 'src/lib/whatever/Form.svelte',
    },
    // An input type with no house primitive stays out of scope.
    {
      code: `<input type="number" bind:value={count} />`,
      filename: 'src/lib/whatever/Form.svelte',
    },
    {
      code: `<input type="color" bind:value={swatch} />`,
      filename: 'src/lib/whatever/Form.svelte',
    },
    // Rendering the text primitives is the intended path.
    {
      code: `<TextInput bind:value={name} ariaLabel="Name" />`,
      filename: 'src/lib/whatever/Form.svelte',
    },
    {
      code: `<TextArea bind:value={notes} ariaLabel="Notes" />`,
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
    // Container roles and roles with no house primitive stay out of scope.
    {
      code: `<div role="radiogroup" aria-label="Type"><span>x</span></div>`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    {
      code: `<button role="tab" aria-selected={on}>Filename</button>`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    // A dynamic role can't be classified statically, so we don't guess.
    {
      code: `<button role={kind} aria-checked={on}>x</button>`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    // A control role on a component is the primitive's own internals
    // (`<Switch.HiddenInput role="switch">`), never a hand-rolled control.
    {
      code: `<Switch.HiddenInput role="switch" aria-label="Tail" />`,
      filename: 'src/lib/ui/Switch.svelte',
    },
    // We only look at `<button>` / `<div>` hosts; a role elsewhere is too rare
    // to guess at.
    {
      code: `<span role="switch" aria-checked={on}>x</span>`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    // A lone info glyph that ISN'T a button is a decorative or status marker,
    // not an info tip: `AdbHint`'s banner glyph and `TransferErrorDialog`'s
    // header glyph both look like this, and both must stay untouched.
    {
      code: `<span class="hint-icon"><Icon name="info" size={13} aria-hidden="true" /></span>`,
      filename: 'src/lib/adb/AdbHint.svelte',
    },
    {
      code: `<Icon name="info" size={22} />`,
      filename: 'src/lib/whatever/Dialog.svelte',
    },
    // A button with a visible label alongside the glyph is a labelled action,
    // not a bare info affordance.
    {
      code: `<button type="button"><Icon name="info" size={14} /> Learn more</button>`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    {
      code: `<button type="button"><Icon name="info" size={14} /><span>Why?</span></button>`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    // Any other glyph in a bare icon button is an ordinary icon action.
    {
      code: `<button type="button" aria-label="Star"><Icon name="star" size={14} /></button>`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    // A dynamic glyph name can't be classified statically, so we don't guess.
    {
      code: `<button type="button"><Icon name={glyph} size={14} /></button>`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    // Rendering the primitive is the intended path.
    {
      code: `<InfoTip label="Analytics" text="What we collect." />`,
      filename: 'src/lib/whatever/Row.svelte',
    },
    // (The per-element opt-out comment is exercised end-to-end by the real
    // eslint config against the bespoke source sites, not here: RuleTester
    // registers the rule under a `rule-to-test/*` id, so a `cmdr/*` disable
    // directive can't match inside the harness.)
  ],
  invalid: [
    // Raw text input → TextInput.
    {
      code: `<input type="text" bind:value={name} />`,
      filename: 'src/lib/whatever/Form.svelte',
      errors: [
        {
          messageId: 'preferPrimitive',
          data: { control: '<input type="text">', primitive: 'TextInput', path: '$lib/ui/TextInput.svelte' },
        },
      ],
    },
    // A TYPELESS input defaults to `text`, so the control kind IS known: flag it.
    // (This is the case the old `undefined`-for-both `staticTypeOf` couldn't see.)
    {
      code: `<input bind:value={name} />`,
      filename: 'src/lib/whatever/Form.svelte',
      errors: [
        {
          messageId: 'preferPrimitive',
          data: { control: '<input>', primitive: 'TextInput', path: '$lib/ui/TextInput.svelte' },
        },
      ],
    },
    // The other text-ish types route to the same primitive.
    {
      code: `<input type="password" bind:value={secret} />`,
      filename: 'src/lib/whatever/Form.svelte',
      errors: [
        {
          messageId: 'preferPrimitive',
          data: { control: '<input type="password">', primitive: 'TextInput', path: '$lib/ui/TextInput.svelte' },
        },
      ],
    },
    {
      code: `<input type="search" bind:value={query} />`,
      filename: 'src/lib/whatever/Form.svelte',
      errors: [
        {
          messageId: 'preferPrimitive',
          data: { control: '<input type="search">', primitive: 'TextInput', path: '$lib/ui/TextInput.svelte' },
        },
      ],
    },
    // Raw textarea → TextArea.
    {
      code: `<textarea bind:value={notes}></textarea>`,
      filename: 'src/lib/whatever/Form.svelte',
      errors: [
        {
          messageId: 'preferPrimitive',
          data: { control: '<textarea>', primitive: 'TextArea', path: '$lib/ui/TextArea.svelte' },
        },
      ],
    },
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
    // Hand-rolled switch → Switch. This is the shape the rule couldn't see
    // before: no native control anywhere, just a button wearing the role.
    {
      code: `<button type="button" role="switch" aria-checked={on}>Count only</button>`,
      filename: 'src/lib/whatever/Row.svelte',
      errors: [
        {
          messageId: 'preferPrimitiveForRole',
          data: { element: 'button', role: 'switch', primitive: 'Switch', path: '$lib/ui/Switch.svelte' },
        },
      ],
    },
    // A `<div>` host counts too.
    {
      code: `<div role="checkbox" aria-checked={on}>x</div>`,
      filename: 'src/lib/whatever/Row.svelte',
      errors: [
        {
          messageId: 'preferPrimitiveForRole',
          data: { element: 'div', role: 'checkbox', primitive: 'Checkbox', path: '$lib/ui/Checkbox.svelte' },
        },
      ],
    },
    // Hand-rolled radio → RadioGroup.
    {
      code: `<button type="button" role="radio" aria-checked={on}>Files</button>`,
      filename: 'src/lib/whatever/Row.svelte',
      errors: [
        {
          messageId: 'preferPrimitiveForRole',
          data: { element: 'button', role: 'radio', primitive: 'RadioGroup', path: '$lib/ui/RadioGroup.svelte' },
        },
      ],
    },
    // Single-quoted static role is still a static literal.
    {
      code: `<button type="button" role='switch' aria-checked={on}>x</button>`,
      filename: 'src/lib/whatever/Row.svelte',
      errors: [
        {
          messageId: 'preferPrimitiveForRole',
          data: { element: 'button', role: 'switch', primitive: 'Switch', path: '$lib/ui/Switch.svelte' },
        },
      ],
    },
    // The role check wins over the element table when both could match: a
    // `<div role="radio">` is a hand-rolled radio, not a native control.
    {
      code: `<div role="radio" aria-checked={on}><input type="checkbox" /></div>`,
      filename: 'src/lib/whatever/Row.svelte',
      errors: [
        {
          messageId: 'preferPrimitiveForRole',
          data: { element: 'div', role: 'radio', primitive: 'RadioGroup', path: '$lib/ui/RadioGroup.svelte' },
        },
        {
          messageId: 'preferPrimitive',
          data: { control: '<input type="checkbox">', primitive: 'Checkbox', path: '$lib/ui/Checkbox.svelte' },
        },
      ],
    },
    // A hand-rolled info tip: a bare `<button>` whose only meaningful child is
    // the info glyph. This is the `StepAi` shape, tooltip wiring and all.
    {
      code: `<button type="button" class="choice-info" aria-label={label} use:tooltip={{ contentEl }}><Icon name="info" size={14} aria-hidden="true" /></button>`,
      filename: 'src/lib/onboarding/StepAi.svelte',
      errors: [
        {
          messageId: 'preferPrimitiveForGlyph',
          data: { icon: 'info', primitive: 'InfoTip', path: '$lib/ui/InfoTip.svelte' },
        },
      ],
    },
    // Whitespace and newlines around the glyph don't make it a second child.
    {
      code: `<button type="button" aria-label="More">\n    <Icon name="info" size={14} />\n</button>`,
      filename: 'src/lib/whatever/Row.svelte',
      errors: [
        {
          messageId: 'preferPrimitiveForGlyph',
          data: { icon: 'info', primitive: 'InfoTip', path: '$lib/ui/InfoTip.svelte' },
        },
      ],
    },
    // A comment beside the glyph isn't a second child either.
    {
      code: `<button type="button" aria-label="More"><!-- why --><Icon name="info" size={14} /></button>`,
      filename: 'src/lib/whatever/Row.svelte',
      errors: [
        {
          messageId: 'preferPrimitiveForGlyph',
          data: { icon: 'info', primitive: 'InfoTip', path: '$lib/ui/InfoTip.svelte' },
        },
      ],
    },
  ],
})
