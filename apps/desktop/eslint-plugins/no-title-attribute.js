/**
 * ESLint rule: route hover hints through the house tooltip action, never the
 * native `title` attribute.
 *
 * Rationale: a native `title` is the browser's tooltip, and it's wrong for Cmdr
 * in four ways:
 *
 *   1. It never appears on keyboard focus. Only a hovering mouse summons it, so
 *      in a keyboard-first app the hint may as well not exist. `use:tooltip`
 *      binds `focus`/`blur` alongside `mouseenter`/`mouseleave`.
 *   2. Its delay is the engine's (roughly 1-2 s) and can't be tuned, so it
 *      lands about a second after ours. Two hints one span apart in the same
 *      row, appearing a second apart, reads as a bug.
 *   3. It's OS chrome, so it ignores our dark/light tokens, type, and
 *      reduced-motion preference.
 *   4. Screen readers announce `title` inconsistently. `use:tooltip` wires
 *      `aria-describedby` to the live tooltip element instead.
 *
 * The action also can't be dismissed with Esc, positioned, or given a
 * `shortcut` / `overflowOnly` / rich-content payload when it's a native title.
 *
 *   <span use:tooltip={tString('...')}>       // instead of title={...}
 *   import { tooltip } from '$lib/tooltip/tooltip'
 *
 * ## What this rule catches
 *
 * A `title` attribute (static, dynamic, or shorthand) on a literal HTML element
 * that isn't in `TITLE_IS_ACCESSIBLE_NAME` below.
 *
 * ## What it deliberately does NOT catch
 *
 * - `title` on a COMPONENT (`<SettingsSection title={...}>`, `<AlertDialog
 *   title={...}>`). Those are ordinary props that happen to share the name, and
 *   they're `kind !== 'html'`, so they never reach the check. This is the bulk
 *   of `title=` in the tree, which is why the rule keys on element kind first.
 * - Embedded content: `<iframe>`, `<embed>`, and `<object>` take their
 *   ACCESSIBLE NAME from `title` (axe's `frame-title`), so removing it would
 *   trade a lint pass for an a11y failure. A tooltip action can't substitute:
 *   it sets `aria-describedby`, which describes a named element rather than
 *   naming it.
 * - `<abbr>` / `<dfn>`, where `title` carries the expansion of the term. That's
 *   semantic content the AT reads as part of the word, not a hover hint.
 * - A spread (`{...props}`) that might carry a `title` through. It can't be
 *   resolved statically, and the component-prop case makes false positives
 *   likely.
 *
 * Opt out per-element when a native title is genuinely the right tool:
 *
 *   <!-- eslint-disable-next-line cmdr/no-title-attribute -- <reason> -->
 */

// Elements whose `title` IS the accessible name or the semantic expansion, so
// it must stay. Everything else uses `title` only as a hover hint.
const TITLE_IS_ACCESSIBLE_NAME = ['iframe', 'embed', 'object', 'abbr', 'dfn']

/**
 * Find a named attribute on a Svelte element, in either spelling:
 * `title="x"` / `title={x}` (`SvelteAttribute`) and `{title}`
 * (`SvelteShorthandAttribute`). Unlike `prefer-ui-primitive`, the VALUE never
 * matters here: a title is a title whether it's static or dynamic, so a dynamic
 * one is flagged rather than skipped.
 */
function attributeNamed(node, name) {
  return node.startTag.attributes.find(
    (candidate) =>
      (candidate.type === 'SvelteAttribute' || candidate.type === 'SvelteShorthandAttribute') &&
      candidate.key?.name === name,
  )
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: 'Use the house tooltip action instead of the native `title` attribute.',
      recommended: true,
    },
    messages: {
      noTitleAttribute:
        'Use the house tooltip action instead of a native `title` on `<{{ element }}>`: ' +
        "`import { tooltip } from '$lib/tooltip/tooltip'`, then `use:tooltip={...}`. A native title never shows on " +
        'keyboard focus, lands about a second after ours, ignores our theme tokens, and is announced inconsistently ' +
        'by screen readers. Keep any `aria-label` the element already has: the action wires `aria-describedby`, so ' +
        'it describes the element rather than naming it. See `lib/ui/CLAUDE.md`. If a native title is genuinely ' +
        'right here, opt out per-element: ' +
        '`<!-- eslint-disable-next-line cmdr/no-title-attribute -- <reason> -->`.',
    },
    schema: [],
  },
  create(context) {
    return {
      SvelteElement(node) {
        // A component's `title` is an ordinary prop, not the HTML attribute.
        if (node.kind !== 'html') return
        const elementName = node.name?.name
        if (!elementName || TITLE_IS_ACCESSIBLE_NAME.includes(elementName)) return
        if (!attributeNamed(node, 'title')) return

        // Report on the start tag (not the attribute) so an
        // `<!-- eslint-disable-next-line ... -->` comment right above the
        // element can suppress it — comments can't live inside a tag.
        context.report({
          node: node.startTag,
          messageId: 'noTitleAttribute',
          data: { element: elementName },
        })
      },
    }
  },
}
