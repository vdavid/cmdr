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

```svelte
<script lang="ts">
    import IconTriangleAlert from '~icons/lucide/triangle-alert'
</script>

<!-- Basic usage: inherits parent text color -->
<IconTriangleAlert />

<!-- With explicit size (use px props, not em) -->
<IconTriangleAlert width="12" height="12" />
```

For styling (color, layout), wrap the icon in a `<span>` with a scoped CSS class. Applying a parent's scoped class
directly to a component's root can be brittle; the wrapping span keeps the usual scoped-style semantics.

```svelte
<span class="my-icon"><IconCircleAlert width="12" height="12" /></span>

<style>
    .my-icon {
        display: inline-flex;
        color: var(--color-warning);
    }
</style>
```

## Sizing

Pass explicit `width` / `height` props (in px) on the icon. Don't use `em`: sizing should be predictable and not float
with surrounding text size.

## Coloring

Icons use `currentColor` by default (they inherit the parent's text color). To color an icon:

- **Preferred**: Set `color` on the parent element (a wrapping `<span>` with a scoped CSS class)
- **For accent color**: Use a scoped class with a stylelint disable comment (because `color: var(--color-accent)` is
  disallowed by default for a11y reasons, as it has insufficient contrast as text):
  ```css
  .my-icon {
    /* stylelint-disable-next-line declaration-property-value-disallowed-list -- icon indicator, not body text */
    color: var(--color-accent);
  }
  ```
- **For semantic colors**: Use `var(--color-warning)`, `var(--color-error)`, etc. directly (these aren't restricted)

## Adding a new icon set

If Lucide doesn't have what you need, install another Iconify set (for example, `pnpm add -D @iconify-json/mdi` for
Material Design Icons). Import from `~icons/mdi/{icon-name}`. Prefer sticking to one set per context for visual
consistency.

## Checklist for adding a new icon

1. Find the icon at [icones.js.org](https://icones.js.org/) in the Lucide collection
2. Import it: `import IconName from '~icons/lucide/{icon-name}'`
3. Render it: `<IconName width="16" height="16" />` (or wrap in a `<span>` with a scoped class for color/layout)
