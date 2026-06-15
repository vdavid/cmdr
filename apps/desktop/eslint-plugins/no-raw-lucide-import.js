/**
 * ESLint rule: ban direct `~icons/lucide/*` (and any `~icons/*` Iconify-set)
 * imports outside the shared glyph registry.
 *
 * Rationale: every inline glyph renders through `<Icon name=… />`
 * (`src/lib/ui/Icon.svelte`), which resolves the glyph from the single registry
 * `src/lib/ui/icons/icon-map.ts`. That registry is the ONE place `~icons/*` is
 * imported. Routing every glyph through it keeps the app on one shared icon
 * style, gives a typed `IconName` union, and makes the Debug "Graphics" catalog
 * complete by construction. A stray `import IconFoo from '~icons/lucide/foo'` in
 * feature code reintroduces an un-catalogued, off-registry glyph.
 *
 * Mirrors `cmdr/no-raw-tauri-invoke`: keep a cross-cutting concern funnelled
 * through its single typed entry point.
 *
 * Opt out per-line if you truly must (you almost never should):
 *
 *   // eslint-disable-next-line cmdr/no-raw-lucide-import -- <reason>
 */

// Path fragments that opt a file out entirely: the registry dir is where the
// glyphs are legitimately imported (`icon-map.ts`, plus any custom-glyph
// components colocated there).
const allowedPathFragments = ['/lib/ui/icons/']

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Render glyphs via `<Icon>`; register them in `lib/ui/icons/icon-map.ts` instead of importing `~icons/*` directly.',
      recommended: true,
    },
    messages: {
      rawLucide:
        'Don\'t import `{{ source }}` in feature code. Render glyphs via `<Icon name="…" />` ' +
        '(`$lib/ui/Icon.svelte`) and add the glyph to the registry `lib/ui/icons/icon-map.ts` (the one ' +
        'place `~icons/*` is imported). See `docs/guides/icons.md`.',
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
        if (!source.value.startsWith('~icons/')) return

        context.report({
          node: source,
          messageId: 'rawLucide',
          data: { source: source.value },
        })
      },
    }
  },
}
