/**
 * ESLint rule: steer feature code to the house UI primitives instead of raw
 * native form controls.
 *
 * Rationale: a raw `<input type="checkbox">`, `<input type="radio">`,
 * `<select>`, `<dialog>`, or `<progress>` looks and behaves like a stock macOS
 * control, which is wrong for Cmdr in three ways:
 *
 *   1. It grays out on window blur (the OS dims background-window controls),
 *      so it flickers dead every time focus leaves the app.
 *   2. It can't be themed. Native controls ignore our dark/light tokens, accent
 *      color, and reduced-motion preferences.
 *   3. It duplicates a11y wiring (labelling, roles, keyboard handling, focus
 *      management) that the matching primitive already owns and tests.
 *
 * The house primitives (`$lib/ui/Checkbox.svelte`, `RadioGroup.svelte`,
 * `Select.svelte`, `ModalDialog.svelte`, `ProgressBar.svelte`) render through
 * Ark UI with our tokens, so every consumer stays consistent by construction.
 * This rule makes new raw controls unable to slip in.
 *
 * ## What this rule catches
 *
 * A raw native control element that has a house-primitive replacement:
 *
 *   - `<input type="checkbox">`  → `Checkbox`
 *   - `<input type="radio">`     → `RadioGroup`
 *   - `<select>`                 → `Select`
 *   - `<dialog>`                 → `ModalDialog`
 *   - `<progress>`               → `ProgressBar`
 *
 * The mapping is a plain table (`MAPPINGS` below); add a row to cover a new
 * primitive.
 *
 * ## What it deliberately does NOT catch
 *
 * - Dynamic `<input type={x}>`: the control kind can't be resolved statically,
 *   so we skip it (mirrors how `dialog-needs-focus-trap` skips dynamic roles).
 *   A typeless `<input>` (defaults to text) is likewise out of scope.
 * - Controls rendered by the primitives themselves. `Checkbox` / `RadioGroup`
 *   render Ark UI's `HiddenInput` (a component, not a literal `<input>`), so
 *   the primitives contain no literal raw control and need no exception here.
 *
 * Opt out per-element for a genuinely bespoke raw control (for example the
 * onboarding radio-cards and the appearance color-swatch picker, whose
 * per-option visuals a plain option list can't express and which carry their
 * own a11y):
 *
 *   <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- <reason> -->
 */

// Element + optional static-`type` predicate → primitive, import path, and the
// human control label used in the message. Extend by adding a row.
const MAPPINGS = [
  {
    element: 'input',
    type: 'checkbox',
    control: '<input type="checkbox">',
    primitive: 'Checkbox',
    path: '$lib/ui/Checkbox.svelte',
  },
  {
    element: 'input',
    type: 'radio',
    control: '<input type="radio">',
    primitive: 'RadioGroup',
    path: '$lib/ui/RadioGroup.svelte',
  },
  { element: 'select', control: '<select>', primitive: 'Select', path: '$lib/ui/Select.svelte' },
  { element: 'dialog', control: '<dialog>', primitive: 'ModalDialog', path: '$lib/ui/ModalDialog.svelte' },
  { element: 'progress', control: '<progress>', primitive: 'ProgressBar', path: '$lib/ui/ProgressBar.svelte' },
]

/**
 * Resolve a Svelte element's static `type` attribute to its literal string, or
 * `undefined` when there's no `type` attribute or it's dynamic (`type={x}`).
 */
function staticTypeOf(node) {
  const typeAttribute = node.startTag.attributes.find(
    (attribute) => attribute.type === 'SvelteAttribute' && attribute.key.name === 'type',
  )
  if (!typeAttribute) return undefined
  const value = typeAttribute.value
  // A single static text chunk counts; `{type}` / `type={x}` are dynamic.
  return value.length === 1 && value[0].type === 'SvelteLiteral' ? value[0].value : undefined
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: 'Use the house UI primitives instead of raw native form controls.',
      recommended: true,
    },
    messages: {
      preferPrimitive:
        'Use the house `{{ primitive }}` primitive (`{{ path }}`) instead of a raw `{{ control }}`. Raw native ' +
        'controls gray out on window blur, ignore our theme tokens, and re-implement a11y wiring the primitive ' +
        'already owns. Browse the primitives in Debug > Components and see `docs/design-system.md`. If a bespoke ' +
        'raw control is genuinely needed, opt out per-element: ' +
        '`<!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- <reason> -->`.',
    },
    schema: [],
  },
  create(context) {
    return {
      SvelteElement(node) {
        if (node.kind !== 'html') return
        const elementName = node.name?.name
        if (!elementName) return

        const candidates = MAPPINGS.filter((mapping) => mapping.element === elementName)
        if (candidates.length === 0) return

        // Rows with a `type` predicate need the element's static `type`. If the
        // type is dynamic (or absent), we can't classify the control: skip.
        const needsType = candidates.some((mapping) => mapping.type !== undefined)
        const staticType = needsType ? staticTypeOf(node) : undefined
        if (needsType && staticType === undefined) return

        const match = candidates.find((mapping) => mapping.type === undefined || mapping.type === staticType)
        if (!match) return

        // Report on the start tag (not an attribute) so an
        // `<!-- eslint-disable-next-line ... -->` comment right above the
        // element can suppress it — comments can't live inside a tag.
        context.report({
          node: node.startTag,
          messageId: 'preferPrimitive',
          data: { control: match.control, primitive: match.primitive, path: match.path },
        })
      },
    }
  },
}
