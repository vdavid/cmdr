# Icons

How we pick, import, size, and color UI icons. Read this when adding or restyling an icon.

We use [`unplugin-icons`](https://github.com/unplugin/unplugin-icons) with `@iconify-json/lucide` for UI icons. Each
icon is imported as a Svelte component and rendered as an **inline SVG** (tree-shaken; only the icons you import ship).
The icon data comes from the [Iconify](https://iconify.design/) ecosystem. We currently use the **Lucide** icon set.

## How it works

At build time, `unplugin-icons` turns an import like `~icons/lucide/triangle-alert` into a tiny Svelte component
containing the inline SVG. The SVG uses `stroke="currentColor"`, so the icon inherits the text color of its parent.

## Finding icons

1. Go to [icones.js.org](https://icones.js.org/) and select the **Lucide** collection to stay visually consistent
2. Search by keyword (for example, "warning", "folder", "check")
3. The icon name (for example `triangle-alert`) maps to the import path `~icons/lucide/triangle-alert`
4. **Always pick icons from the same set** (Lucide) for visual cohesion (consistent stroke width and style)

If you're an AI agent looking for icons: search at `https://icones.js.org/collection/lucide?s={search+terms}`, suggest
candidates to the user with the search URL and terms so they can browse and pick, then use the chosen icon name.

## Using icons in templates

Render every glyph through the shared `Icon` component, never by importing `~icons/lucide/*` directly in feature code.
`Icon` picks the glyph by `name` from the central registry (`lib/ui/icons/icon-map.ts`) and renders it at `size` px:

```svelte
<script lang="ts">
    import Icon from '$lib/ui/Icon.svelte'
</script>

<!-- Basic usage: inherits parent text color -->
<Icon name="triangle-alert" size={16} />

<!-- Decorative vs meaningful: a11y attributes pass through to the svg -->
<Icon name="triangle-alert" size={16} aria-hidden="true" />
<Icon name="hourglass" size={12} role="img" aria-label="Size updating" />
```

A dynamic glyph (chosen at runtime) takes a typed name:

```svelte
<script lang="ts">
    import type { IconName } from '$lib/ui/icons/icon-map'
    const glyph: IconName = isBranch ? 'git-branch' : 'tag'
</script>
<Icon name={glyph} size={16} />
```

For styling (color, layout), wrap the icon in a `<span>` with a scoped CSS class. `Icon` has no `color` prop on purpose:
the glyph inherits `currentColor`, so color lives on the wrapper. Don't pass a scoped class to `Icon` directly; a scoped
class on a component root is brittle. Use a wrapping span:

```svelte
<span class="my-icon"><Icon name="circle-alert" size={12} /></span>

<style>
    .my-icon {
        display: inline-flex;
        color: var(--color-warning);
    }
</style>
```

## Sizing

Pass an explicit `size` (px) on `Icon`. Don't rely on `em`: sizing should be predictable and not float with surrounding
text size.

## Coloring

The glyph inherits `currentColor`. To color an icon, set `color` on the wrapping element:

- **Preferred**: Set `color` on the wrapping `<span>` (a scoped CSS class)
- **For accent color**: Use a scoped class with a stylelint disable comment (because `color: var(--color-accent)` is
  disallowed by default for a11y reasons, as it has insufficient contrast as text):
  ```css
  .my-icon {
    /* stylelint-disable-next-line declaration-property-value-disallowed-list -- icon indicator, not body text */
    color: var(--color-accent);
  }
  ```
- **For semantic colors**: Use `var(--color-warning)`, `var(--color-error)`, etc. directly (these aren't restricted)

## Adding a new glyph

1. Find the icon at [icones.js.org](https://icones.js.org/) in the Lucide collection (stay in one set for visual
   cohesion).
2. Add it to the registry `lib/ui/icons/icon-map.ts`: `import IconName from '~icons/lucide/{icon-name}'` at the top, and
   one `'{icon-name}': IconName` entry in `ICON_COMPONENTS`. This is the only place `~icons/lucide/*` is imported.
3. Render it anywhere: `<Icon name="{icon-name}" size={16} />`. The `IconName` union, the Debug "Graphics" catalog
   (`routes/dev/graphics/`), and tests all pick it up automatically.

A custom glyph Lucide doesn't ship (for example `eject`) lives as a small `.svelte` component in `lib/ui/icons/` (a bare
`<svg {...rest}>`) and registers the same way, so it's interchangeable with Lucide glyphs at the call site.

If Lucide lacks what you need and there's no custom glyph, install another Iconify set (for example
`pnpm add -D @iconify-json/mdi`) and import it into the registry from `~icons/mdi/{icon-name}`. Prefer one set per
context.

Every registered glyph appears in the Debug window's **Graphics** catalog (Cmd+Shift+D → Graphics → Icons), with a
tooltip describing where it's used.
