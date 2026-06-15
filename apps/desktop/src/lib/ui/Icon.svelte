<script lang="ts">
    import type { SVGAttributes } from 'svelte/elements'
    import { ICON_COMPONENTS, type IconName } from './icons/icon-map'

    /**
     * The one way to render an inline glyph. Picks the glyph by `name` from the shared registry
     * (`icons/icon-map.ts`) and renders it at `size` px square. Lucide and custom glyphs are
     * interchangeable here, the caller can't tell them apart.
     *
     * Color is intentionally NOT a prop: the glyph inherits `currentColor`, so set `color` on the
     * wrapping element (a `<span>`/`<button>` with a scoped class) the way every call site already
     * does. a11y attributes (`aria-hidden`, `role`, `aria-label`, `title`) pass straight through to
     * the `<svg>` via `{...rest}`, so a decorative icon takes `aria-hidden`, a meaningful one takes
     * `role="img"` + `aria-label`.
     */
    interface Props extends Omit<SVGAttributes<SVGSVGElement>, 'width' | 'height'> {
        name: IconName
        /** Square size in px (both width and height). */
        size?: number
    }

    const { name, size = 16, ...rest }: Props = $props()

    const Glyph = $derived(ICON_COMPONENTS[name])
</script>

<Glyph width={size} height={size} {...rest} />
