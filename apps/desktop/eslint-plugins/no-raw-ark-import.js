/**
 * ESLint rule: ban direct `@ark-ui/svelte` (and `@ark-ui/svelte/*`) imports
 * outside the sanctioned house-wrapper files.
 *
 * Rationale: Cmdr wraps each `@ark-ui/svelte` primitive exactly once in a house
 * component under `lib/ui/` (`Switch`, `Checkbox`, `Select`, `Combobox`,
 * `RadioGroup`, `ToggleGroup`, `Menu`, …), named 1:1 after Ark's component.
 * Feature and section code uses those wrappers, never Ark directly. Funnelling
 * every primitive through its single wrapper keeps one shared style, one a11y
 * implementation, and one place to patch Ark quirks. A stray
 * `import { NumberInput } from '@ark-ui/svelte/number-input'` in a section
 * re-introduces an unwrapped, off-style control.
 *
 * Mirrors `cmdr/no-raw-lucide-import`: keep a cross-cutting concern funnelled
 * through its single typed entry point.
 *
 * Opt out per-line if you truly must (you almost never should):
 *
 *   // eslint-disable-next-line cmdr/no-raw-ark-import -- <reason>
 */

// Path fragments that opt a file out entirely: any file under `lib/ui/` is a
// house primitive and may wrap Ark. Nothing else is exempt. Keep it that way —
// a named exception outside `lib/ui/` means a primitive is missing, so add the
// wrapper instead of widening this list.
const allowedPathFragments = ['/lib/ui/']

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        "Use the house `lib/ui/` wrappers; don't import `@ark-ui/svelte` primitives directly in feature code.",
      recommended: true,
    },
    messages: {
      rawArk:
        "Don't import `{{ source }}` in feature code. Use the house wrapper in `$lib/ui/` (each Ark " +
        'primitive is wrapped there exactly once, named 1:1 after Ark). If no wrapper exists yet, add one in ' +
        '`lib/ui/` rather than reaching for Ark directly. See `src/lib/ui/CLAUDE.md`.',
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename || context.getFilename() || ''
    if (allowedPathFragments.some((fragment) => filename.includes(fragment))) {
      return {}
    }

    return {
      ImportDeclaration(node) {
        const source = node.source
        if (source.type !== 'Literal' || typeof source.value !== 'string') return
        if (source.value !== '@ark-ui/svelte' && !source.value.startsWith('@ark-ui/svelte/')) return

        context.report({
          node: source,
          messageId: 'rawArk',
          data: { source: source.value },
        })
      },
    }
  },
}
