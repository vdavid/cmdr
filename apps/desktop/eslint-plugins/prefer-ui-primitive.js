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
 * 1. A raw native control element that has a house-primitive replacement:
 *
 *   - `<input type="checkbox">`  → `Checkbox`
 *   - `<input type="radio">`     → `RadioGroup`
 *   - `<input>` with a text-ish type, or no type at all → `TextInput`
 *   - `<textarea>`               → `TextArea`
 *   - `<select>`                 → `Select`
 *   - `<dialog>`                 → `ModalDialog`
 *   - `<progress>`               → `ProgressBar`
 *
 * The mapping is a plain table (`MAPPINGS` below); add a row to cover a new
 * primitive.
 *
 * 2. A hand-rolled control: a plain `<button>` or `<div>` wearing the ARIA role
 *    of a control a primitive already owns:
 *
 *   - `role="switch"`   → `Switch`
 *   - `role="checkbox"` → `Checkbox`
 *   - `role="radio"`    → `RadioGroup`
 *
 * These don't gray out on blur (they're not native), but they re-implement the
 * state, keyboard, and focus wiring the primitive already owns and tests, and
 * they drift from its tokens and geometry. That table is `ROLE_MAPPINGS`.
 *
 * 3. A hand-rolled glyph affordance: a `<button>` whose only meaningful child is
 *    one bare glyph a primitive already owns:
 *
 *   - `<Icon name="info">` → `InfoTip`
 *
 * A button with nothing in it but an info glyph IS an info tip, whatever the
 * class on it says, and the primitive already carries the accessible name, the
 * tooltip wiring, the hover and `:focus-visible` treatment, and the glyph size.
 * A clone drifts on every one of those: the eight info glyphs in the tree had
 * grown four different sizes before the primitive existed. That table is
 * `GLYPH_MAPPINGS`.
 *
 * ## What it deliberately does NOT catch
 *
 * - Dynamic `<input type={x}>` / `<button role={r}>`: the control kind can't be
 *   resolved statically, so we skip it (mirrors how `dialog-needs-focus-trap`
 *   skips dynamic roles). A TYPELESS `<input>` is the opposite case and IS
 *   flagged: HTML defaults it to `text`, so the control kind is known.
 * - Input types with no house primitive (`number`, `color`, `file`, `range`,
 *   `date`, …): they simply have no `MAPPINGS` row.
 * - Container roles (`role="radiogroup"`, `role="tablist"`) and roles with no
 *   house primitive (`role="tab"`, `role="option"`). Only the leaf control
 *   roles in `ROLE_MAPPINGS` are flagged.
 * - Roles on a component (`<Switch.HiddenInput role="switch">`): components are
 *   `kind !== 'html'`, so the primitives' own internals never self-flag.
 * - Controls rendered by the primitives themselves. `Checkbox` / `RadioGroup`
 *   render Ark UI's `HiddenInput` (a component, not a literal `<input>`), so
 *   they need no exception. `TextInput` / `TextArea` DO render the literal
 *   element they replace, so they carry a file-level opt-out in `eslint.config.js`.
 *   `InfoTip` is the same case and carries one too.
 * - A glyph that isn't in a `<button>`. `<span><Icon name="info" /></span>` is a
 *   decorative or status marker, not an affordance: `AdbHint`'s banner glyph and
 *   `TransferErrorDialog`'s header glyph are both that, and the restricted /
 *   symlink markers in the file lists are `StatusMarker`. Only a real button is
 *   the interactive thing `InfoTip` replaces.
 * - A button that also carries a visible label, another element, or a second
 *   glyph. Then it's an ordinary labelled action that happens to lead with a
 *   glyph, and no primitive owns that shape.
 * - A dynamic glyph name (`<Icon name={glyph} />`): unresolvable statically, the
 *   same way a dynamic `type` / `role` is skipped above.
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
  // Text-ish inputs, including a typeless one (HTML defaults it to `text`).
  // `null` is the "no `type` attribute" marker; see `staticAttributeOf`.
  ...[null, 'text', 'password', 'email', 'search', 'url', 'tel'].map((type) => ({
    element: 'input',
    type,
    control: type === null ? '<input>' : `<input type="${type}">`,
    primitive: 'TextInput',
    path: '$lib/ui/TextInput.svelte',
  })),
  { element: 'textarea', control: '<textarea>', primitive: 'TextArea', path: '$lib/ui/TextArea.svelte' },
  { element: 'select', control: '<select>', primitive: 'Select', path: '$lib/ui/Select.svelte' },
  { element: 'dialog', control: '<dialog>', primitive: 'ModalDialog', path: '$lib/ui/ModalDialog.svelte' },
  { element: 'progress', control: '<progress>', primitive: 'ProgressBar', path: '$lib/ui/ProgressBar.svelte' },
]

// Elements that can host a hand-rolled control role. A role on anything else
// (a `<span role="switch">`, say) is rare enough that we'd rather not guess.
const ROLE_HOSTS = ['button', 'div']

// Leaf ARIA control role → the primitive that already implements it. Container
// roles (`radiogroup`, `tablist`) and roles with no primitive (`tab`, `option`)
// stay out. Extend by adding a row.
const ROLE_MAPPINGS = [
  { role: 'switch', primitive: 'Switch', path: '$lib/ui/Switch.svelte' },
  { role: 'checkbox', primitive: 'Checkbox', path: '$lib/ui/Checkbox.svelte' },
  { role: 'radio', primitive: 'RadioGroup', path: '$lib/ui/RadioGroup.svelte' },
]

// `<Icon name>` glyph → the primitive that owns a `<button>` wrapping it alone.
// Extend by adding a row when a new bare-glyph affordance gets a primitive.
const GLYPH_MAPPINGS = [{ icon: 'info', primitive: 'InfoTip', path: '$lib/ui/InfoTip.svelte' }]

/**
 * Resolve a Svelte element's named static attribute. Three outcomes, and keeping
 * them apart is what lets a typeless `<input>` be flagged while a dynamic
 * `<input type={x}>` is skipped:
 *
 *   - the literal string, for a static attribute;
 *   - `null`, when the attribute is ABSENT (so the element's default applies);
 *   - `undefined`, when it's DYNAMIC (`type={x}`) and can't be resolved.
 */
function staticAttributeOf(node, name) {
  const attribute = node.startTag.attributes.find(
    (candidate) => candidate.type === 'SvelteAttribute' && candidate.key.name === name,
  )
  if (!attribute) return null
  const value = attribute.value
  // A single static text chunk counts; `{type}` / `type={x}` are dynamic.
  return value.length === 1 && value[0].type === 'SvelteLiteral' ? value[0].value : undefined
}

/**
 * The `<Icon name>` of the ONE glyph a `<button>` wraps, or `null` when the
 * button holds anything else. Whitespace between tags and template comments
 * don't count as children: a glyph on its own line is still a lone glyph.
 */
function loneGlyphOf(node) {
  const meaningful = node.children.filter((child) =>
    child.type === 'SvelteText' ? child.value.trim() !== '' : child.type !== 'SvelteHTMLComment',
  )
  if (meaningful.length !== 1) return null
  const only = meaningful[0]
  // `kind === 'html'` would be a literal `<icon>` element, not our component.
  if (only.type !== 'SvelteElement' || only.kind === 'html' || only.name?.name !== 'Icon') return null
  return staticAttributeOf(only, 'name')
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
      preferPrimitiveForRole:
        'Use the house `{{ primitive }}` primitive (`{{ path }}`) instead of a hand-rolled ' +
        '`<{{ element }} role="{{ role }}">`. The primitive already owns and tests the state, keyboard, and focus ' +
        'wiring this role promises, and keeps the tokens and geometry consistent. Browse the primitives in ' +
        'Debug > Components and see `docs/design-system.md`. If a bespoke control is genuinely needed, opt out ' +
        'per-element: `<!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- <reason> -->`.',
      preferPrimitiveForGlyph:
        'Use the house `{{ primitive }}` primitive (`{{ path }}`) instead of a `<button>` wrapping a bare ' +
        '`<Icon name="{{ icon }}">`. The primitive already owns the accessible name, the tooltip wiring (plain text ' +
        'or a rich snippet), the hover and `:focus-visible` treatment, and the glyph size, all of which a clone ' +
        'drifts from. Browse the primitives in Debug > Components and see `docs/design-system.md`. A glyph that ' +
        "isn't an affordance belongs in a `<span>` (decorative) or in `StatusMarker` (a status marker), not in a " +
        'button. If a bespoke button is genuinely needed, opt out per-element: ' +
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

        // A `<button>` / `<div>` wearing a leaf control role is a hand-rolled
        // control, reported against the role table rather than the element one.
        if (ROLE_HOSTS.includes(elementName)) {
          const role = ROLE_MAPPINGS.find((mapping) => mapping.role === staticAttributeOf(node, 'role'))
          if (role) {
            context.report({
              node: node.startTag,
              messageId: 'preferPrimitiveForRole',
              data: { element: elementName, role: role.role, primitive: role.primitive, path: role.path },
            })
            return
          }
        }

        // A `<button>` holding nothing but one glyph is a hand-rolled version of
        // whichever primitive owns that glyph.
        if (elementName === 'button') {
          const glyph = GLYPH_MAPPINGS.find((mapping) => mapping.icon === loneGlyphOf(node))
          if (glyph) {
            context.report({
              node: node.startTag,
              messageId: 'preferPrimitiveForGlyph',
              data: { icon: glyph.icon, primitive: glyph.primitive, path: glyph.path },
            })
            return
          }
        }

        const candidates = MAPPINGS.filter((mapping) => mapping.element === elementName)
        if (candidates.length === 0) return

        // Rows with a `type` predicate need the element's `type`. `undefined`
        // means DYNAMIC (`type={x}`), which we can't classify: skip. `null`
        // means ABSENT, which matches the typeless row (defaults to text).
        const needsType = candidates.some((mapping) => mapping.type !== undefined)
        const staticType = needsType ? staticAttributeOf(node, 'type') : undefined
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
