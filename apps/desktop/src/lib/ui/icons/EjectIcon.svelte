<script lang="ts">
    import type { SVGAttributes } from 'svelte/elements'

    /**
     * Custom eject glyph. Lucide doesn't ship an `eject` icon (as of v0.477), and the shape matches
     * the macOS menu-bar eject glyph: a filled triangle above a short bar. Authored as a component so
     * it resolves through the shared `Icon` registry like any Lucide glyph (`<Icon name="eject" />`).
     * Spreads `width`/`height`/aria attributes from `Icon` straight onto the `<svg>`.
     */
    const rest: SVGAttributes<SVGSVGElement> = $props()
</script>

<svg
    viewBox="0 0 24 24"
    fill="currentColor"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    {...rest}
>
    <path d="M5 14 L12 5 L19 14 Z" />
    <line x1="5" y1="19" x2="19" y2="19" />
</svg>
