<!--
  Finder-tag dot cluster shown at the right edge of a file's Name cell.

  Pure presentational: the overflow/cap/colour logic lives in `tag-dots-utils.ts`
  (unit-tested without rendering). Renders only colored tags (index 1-7);
  colourless tags (index 0) produce no dot but still appear in the accessible
  label. Dots overlap ForkLift-style with the leftmost on top; past three
  colored tags, two dots plus a faint `+N` chip.
-->
<script lang="ts">
    import type { TagRef } from '$lib/ipc/bindings'
    import { formatInteger } from '$lib/intl/number-format'
    import { tagDotsModel, tagColorVar } from './tag-dots-utils'

    interface Props {
        tags: TagRef[] | undefined
    }

    const { tags }: Props = $props()

    const model = $derived(tagDotsModel(tags))
</script>

{#if model.dots.length > 0}
    <span class="tag-dots" role="img" aria-label={model.label} title={model.label}>
        {#each model.dots as dot, i (i)}
            <span
                class="tag-dot"
                style="background-color: {tagColorVar(dot.color)}; z-index: {model.dots.length - i};"
                aria-hidden="true"
            ></span>
        {/each}
        {#if model.overflowCount > 0}
            <span class="tag-chip" aria-hidden="true">+{formatInteger(model.overflowCount)}</span>
        {/if}
    </span>
{/if}

<style>
    /* The cluster reserves its width via `tagClusterWidthPx`; keep these values
       in sync with the constants in `tag-dots-utils.ts`. Decorative, so it never
       eats row clicks. */
    .tag-dots {
        display: inline-flex;
        align-items: center;
        flex-shrink: 0;
        /* TAG_CLUSTER_GAP */
        margin-left: 5px;
        pointer-events: none;
    }

    .tag-dot {
        position: relative;
        /* TAG_DOT_SIZE */
        width: 10px;
        height: 10px;
        box-sizing: border-box;
        border-radius: var(--radius-full);
        border: 1px solid var(--color-tag-border);
    }

    /* Overlap each subsequent dot by TAG_DOT_SIZE - TAG_DOT_OVERLAP_OFFSET (5px).
       Inline z-index keeps the leftmost dot on top (Finder order). */
    .tag-dot:not(:first-child),
    .tag-chip {
        margin-left: -5px;
    }

    .tag-chip {
        position: relative;
        z-index: 0;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        height: 10px;
        min-width: 16px;
        padding: 0 3px;
        box-sizing: border-box;
        border-radius: var(--radius-sm);
        border: 1px solid var(--color-tag-border);
        background-color: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
        /* Tiny overflow badge; below the smallest font-size token, so raw px. */
        font-size: 8px;
        line-height: 1;
        font-variant-numeric: tabular-nums;
    }
</style>
