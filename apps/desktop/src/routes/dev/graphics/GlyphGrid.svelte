<script lang="ts" generics="T extends { id: string; caption: string; usage: string }">
    /**
     * Review grid for the Graphics catalog: one captioned cell per glyph, each with a
     * tooltip saying where it shows up in the app. The section supplies the intro copy
     * and how to draw one glyph; the grid owns the layout and the 48px host every glyph
     * sits in.
     */
    import type { Snippet } from 'svelte'
    import { tooltip } from '$lib/tooltip/tooltip'

    interface Props {
        items: readonly T[]
        intro: Snippet
        /** Draws one glyph at the shared 24px review size. */
        glyph: Snippet<[T]>
    }

    const { items, intro, glyph }: Props = $props()
</script>

<p class="intro">{@render intro()}</p>
<div class="grid">
    {#each items as item (item.id)}
        <div class="cell" use:tooltip={item.usage}>
            <div class="glyph-host">
                {@render glyph(item)}
            </div>
            <p class="caption">{item.caption}</p>
        </div>
    {/each}
</div>

<style>
    .intro {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
        gap: var(--spacing-lg);
    }

    .cell {
        display: flex;
        flex-direction: column;
        align-items: center;
    }

    .glyph-host {
        height: 48px;
        display: flex;
        align-items: center;
        color: var(--color-text-primary);
    }

    .caption {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
        color: var(--color-text-tertiary);
        text-align: center;
    }
</style>
